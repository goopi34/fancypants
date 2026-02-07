mod ble;
mod config;
mod mapper;
mod toy;

use clap::Parser;
use config::Config;
use mapper::RangeMapper;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(
    name = "fancypants",
    about = "BLE rangefinder to Buttplug.io middleware",
    long_about = "Connects to a fancypants-nrf52 BLE rangefinder and translates distance \
                   readings into haptic intensity for toys via Intiface Engine."
)]
struct Args {
    /// Path to TOML configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Generate a default config file and exit
    #[arg(long)]
    generate_config: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Set up logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&args.log_level)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Generate default config if requested
    if args.generate_config {
        Config::save_default(&args.config)?;
        info!("Default config written to {:?}", args.config);
        return Ok(());
    }

    // Load config
    let config = if args.config.exists() {
        Config::load(&args.config)?
    } else {
        warn!("Config file {:?} not found, using defaults", args.config);
        Config::default()
    };

    info!("Configuration loaded:");
    info!("  BLE device: {}", config.ble.device_name);
    info!(
        "  Mapping: range [{}-{}mm] -> intensity [{}-{}], invert={}, deadzone={}mm",
        config.mapping.min_range_mm,
        config.mapping.max_range_mm,
        config.mapping.min_intensity,
        config.mapping.max_intensity,
        config.mapping.invert,
        config.mapping.deadzone_mm,
    );
    info!("  Buttplug server: {}", config.buttplug.server_address);

    // Ctrl+C handling
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    ctrlc::set_handler(move || {
        info!("Shutdown requested...");
        running_clone.store(false, Ordering::SeqCst);
    })?;

    // Main loop with reconnection
    while running.load(Ordering::SeqCst) {
        match run_session(&config, &running).await {
            Ok(()) => {
                info!("Session ended cleanly");
                break;
            }
            Err(e) => {
                error!("Session error: {:#}", e);
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                info!(
                    "Reconnecting in {}s...",
                    config.ble.reconnect_delay_secs
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    config.ble.reconnect_delay_secs,
                ))
                .await;
            }
        }
    }

    info!("Goodbye");
    Ok(())
}

async fn run_session(config: &Config, running: &Arc<AtomicBool>) -> anyhow::Result<()> {
    // 1. Find fancypants-nrf52 BLE device
    let peripheral: btleplug::platform::Peripheral =
        ble::find_device(&config.ble.device_name, config.ble.scan_timeout_secs).await?;

    // 2. Connect to Intiface Engine
    let mut toy: toy::ToyController =
        toy::ToyController::connect(&config.buttplug.server_address).await?;
    toy.find_device(config.buttplug.device_index).await?;

    // 3. Set up range mapper
    let mut mapper = RangeMapper::new(config.mapping.clone());

    // 4. Start BLE notification listener
    let (tx, mut rx) = mpsc::unbounded_channel();
    let ble_handle = {
        let peripheral = peripheral.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = ble::run_ble_client(&peripheral, tx).await {
                error!("BLE client error: {:#}", e);
            }
        })
    };

    // 5. Process range updates and drive toy
    info!("Running — move your hand near the sensor!");

    while running.load(Ordering::SeqCst) {
        tokio::select! {
            Some(event) = rx.recv() => {
                match event {
                    ble::BleEvent::RangeUpdate(distance_mm) => {
                        let intensity = mapper.map(distance_mm);
                        if let Err(e) = toy.set_intensity(intensity).await {
                            warn!("Failed to set intensity: {:#}", e);
                        }
                    }
                    ble::BleEvent::Disconnected => {
                        warn!("BLE disconnected");
                        break;
                    }
                    ble::BleEvent::Connected => {
                        info!("BLE connected");
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                // Periodic check that everything is still alive
                if !toy.is_connected() {
                    warn!("Lost connection to Intiface");
                    break;
                }
            }
        }
    }

    // Cleanup
    info!("Stopping device...");
    let _ = toy.stop().await;
    let _ = toy.disconnect().await;
    ble_handle.abort();

    Ok(())
}
