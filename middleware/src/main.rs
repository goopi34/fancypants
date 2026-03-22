mod ble;
mod config;
mod mapper;
mod toy;

use clap::Parser;
use config::Config;
use mapper::RangeMapper;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(
    name = "fancypants",
    version = env!("FANCYPANTS_VERSION"),
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
    let config = load_config(&args.config)?;
    log_config(&config);

    // Ctrl+C handling
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    ctrlc::set_handler(move || {
        info!("Shutdown requested...");
        running_clone.store(false, Ordering::SeqCst);
    })?;

    // Main loop with reconnection
    reconnect_loop(&config, &running, RealSession).await;

    info!("Goodbye");
    Ok(())
}

/// Load configuration from a file, falling back to defaults if not found.
pub(crate) fn load_config(path: &Path) -> anyhow::Result<Config> {
    if path.exists() {
        Config::load(path)
    } else {
        warn!("Config file {:?} not found, using defaults", path);
        Ok(Config::default())
    }
}

/// Log the loaded configuration summary.
pub(crate) fn log_config(config: &Config) {
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
}

/// Reconnect loop: runs sessions until clean exit or shutdown signal.
pub(crate) async fn reconnect_loop(
    config: &Config,
    running: &Arc<AtomicBool>,
    session_fn: impl AsyncSessionFn,
) {
    while running.load(Ordering::SeqCst) {
        match session_fn.run(config, running).await {
            Ok(()) => {
                info!("Session ended cleanly");
                break;
            }
            Err(e) => {
                error!("Session error: {:#}", e);
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                info!("Reconnecting in {}s...", config.ble.reconnect_delay_secs);
                tokio::time::sleep(std::time::Duration::from_secs(
                    config.ble.reconnect_delay_secs,
                ))
                .await;
            }
        }
    }
}

/// Trait for session runner functions, to work around async closure lifetime issues.
#[async_trait::async_trait]
pub(crate) trait AsyncSessionFn {
    async fn run(&self, config: &Config, running: &Arc<AtomicBool>) -> anyhow::Result<()>;
}

struct RealSession;

#[async_trait::async_trait]
impl AsyncSessionFn for RealSession {
    async fn run(&self, config: &Config, running: &Arc<AtomicBool>) -> anyhow::Result<()> {
        run_session(config, running).await
    }
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

    let backend: &mut dyn toy::ToyBackend = &mut toy;
    let result = run_session_inner(backend, &mut rx, &mut mapper, running).await;

    // Cleanup
    info!("Stopping device...");
    let _ = backend.stop().await;
    let _ = backend.disconnect().await;
    ble_handle.abort();

    result
}

/// Core event loop, extracted for testability.
pub(crate) async fn run_session_inner(
    toy: &mut dyn toy::ToyBackend,
    rx: &mut mpsc::UnboundedReceiver<ble::BleEvent>,
    mapper: &mut RangeMapper,
    running: &Arc<AtomicBool>,
) -> anyhow::Result<()> {
    info!("Running — move your hand near the sensor!");

    while running.load(Ordering::SeqCst) {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(ble::BleEvent::RangeUpdate(distance_mm)) => {
                        let intensity = mapper.map(distance_mm);
                        if let Err(e) = toy.set_intensity(intensity).await {
                            warn!("Failed to set intensity: {:#}", e);
                        }
                    }
                    Some(ble::BleEvent::Disconnected) | None => {
                        warn!("BLE disconnected");
                        break;
                    }
                    Some(ble::BleEvent::Connected) => {
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MappingConfig;
    use std::sync::atomic::AtomicU32;

    struct MockToy {
        intensities: Vec<f64>,
        connected: bool,
    }

    impl MockToy {
        fn new() -> Self {
            MockToy {
                intensities: Vec::new(),
                connected: true,
            }
        }
    }

    #[async_trait::async_trait]
    impl toy::ToyBackend for MockToy {
        async fn set_intensity(&mut self, intensity: f64) -> anyhow::Result<()> {
            self.intensities.push(intensity);
            Ok(())
        }

        async fn stop(&mut self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn disconnect(&self) -> anyhow::Result<()> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            self.connected
        }
    }

    fn test_mapping_config() -> MappingConfig {
        MappingConfig {
            invert: true,
            min_range_mm: 30,
            max_range_mm: 300,
            min_intensity: 0.0,
            max_intensity: 1.0,
            deadzone_mm: 500,
            smoothing: 0.0,
        }
    }

    // --- run_session_inner tests ---

    #[tokio::test]
    async fn test_session_processes_range_updates() {
        let mut toy = MockToy::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut mapper = RangeMapper::new(test_mapping_config());
        let running = Arc::new(AtomicBool::new(true));

        tx.send(ble::BleEvent::RangeUpdate(30)).unwrap();
        tx.send(ble::BleEvent::RangeUpdate(300)).unwrap();
        drop(tx);

        run_session_inner(&mut toy, &mut rx, &mut mapper, &running)
            .await
            .unwrap();

        assert_eq!(toy.intensities.len(), 2);
        assert!((toy.intensities[0] - 1.0).abs() < 0.01);
        assert!((toy.intensities[1] - 0.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_session_stops_on_disconnect_event() {
        let mut toy = MockToy::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut mapper = RangeMapper::new(test_mapping_config());
        let running = Arc::new(AtomicBool::new(true));

        tx.send(ble::BleEvent::RangeUpdate(165)).unwrap();
        tx.send(ble::BleEvent::Disconnected).unwrap();
        tx.send(ble::BleEvent::RangeUpdate(30)).unwrap();

        run_session_inner(&mut toy, &mut rx, &mut mapper, &running)
            .await
            .unwrap();

        assert_eq!(toy.intensities.len(), 1);
    }

    #[tokio::test]
    async fn test_session_handles_connected_event() {
        let mut toy = MockToy::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut mapper = RangeMapper::new(test_mapping_config());
        let running = Arc::new(AtomicBool::new(true));

        tx.send(ble::BleEvent::Connected).unwrap();
        tx.send(ble::BleEvent::RangeUpdate(165)).unwrap();
        drop(tx);

        run_session_inner(&mut toy, &mut rx, &mut mapper, &running)
            .await
            .unwrap();

        assert_eq!(toy.intensities.len(), 1);
    }

    #[tokio::test]
    async fn test_session_stops_on_running_false() {
        let mut toy = MockToy::new();
        let (_tx, mut rx) = mpsc::unbounded_channel();
        let mut mapper = RangeMapper::new(test_mapping_config());
        let running = Arc::new(AtomicBool::new(false));

        run_session_inner(&mut toy, &mut rx, &mut mapper, &running)
            .await
            .unwrap();

        assert!(toy.intensities.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_session_stops_on_toy_disconnect() {
        let mut toy = MockToy::new();
        toy.connected = false;
        let (_tx, mut rx) = mpsc::unbounded_channel();
        let mut mapper = RangeMapper::new(test_mapping_config());
        let running = Arc::new(AtomicBool::new(true));

        run_session_inner(&mut toy, &mut rx, &mut mapper, &running)
            .await
            .unwrap();

        assert!(toy.intensities.is_empty());
    }

    // --- load_config tests ---

    #[test]
    fn test_load_config_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        Config::save_default(&path).unwrap();
        let config = load_config(&path).unwrap();
        assert_eq!(config.ble.device_name, "Rangefinder");
    }

    #[test]
    fn test_load_config_missing_uses_defaults() {
        let config = load_config(Path::new("/tmp/nonexistent_fp_config.toml")).unwrap();
        assert_eq!(config.ble.device_name, "Rangefinder");
    }

    // --- log_config test ---

    #[test]
    fn test_log_config_does_not_panic() {
        let config = Config::default();
        log_config(&config);
    }

    // --- reconnect_loop tests ---

    struct MockSession {
        call_count: Arc<AtomicU32>,
        fail_until: u32,
        shutdown_on_call: Option<Arc<AtomicBool>>,
    }

    #[async_trait::async_trait]
    impl AsyncSessionFn for MockSession {
        async fn run(&self, _config: &Config, _running: &Arc<AtomicBool>) -> anyhow::Result<()> {
            let n = self.call_count.fetch_add(1, Ordering::SeqCst);
            if let Some(ref running) = self.shutdown_on_call {
                running.store(false, Ordering::SeqCst);
            }
            if n < self.fail_until {
                anyhow::bail!("simulated error");
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_reconnect_loop_clean_exit() {
        let config = Config::default();
        let running = Arc::new(AtomicBool::new(true));
        let call_count = Arc::new(AtomicU32::new(0));

        reconnect_loop(
            &config,
            &running,
            MockSession {
                call_count: call_count.clone(),
                fail_until: 0,
                shutdown_on_call: None,
            },
        )
        .await;

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_reconnect_loop_retries_on_error() {
        let mut config = Config::default();
        config.ble.reconnect_delay_secs = 0;
        let running = Arc::new(AtomicBool::new(true));
        let call_count = Arc::new(AtomicU32::new(0));

        reconnect_loop(
            &config,
            &running,
            MockSession {
                call_count: call_count.clone(),
                fail_until: 2,
                shutdown_on_call: None,
            },
        )
        .await;

        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_reconnect_loop_stops_on_shutdown() {
        let mut config = Config::default();
        config.ble.reconnect_delay_secs = 0;
        let running = Arc::new(AtomicBool::new(true));
        let call_count = Arc::new(AtomicU32::new(0));

        reconnect_loop(
            &config,
            &running,
            MockSession {
                call_count: call_count.clone(),
                fail_until: 100,
                shutdown_on_call: Some(running.clone()),
            },
        )
        .await;

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    // --- session error handling ---

    struct FailingToy;

    #[async_trait::async_trait]
    impl toy::ToyBackend for FailingToy {
        async fn set_intensity(&mut self, _intensity: f64) -> anyhow::Result<()> {
            anyhow::bail!("device error");
        }

        async fn stop(&mut self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn disconnect(&self) -> anyhow::Result<()> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_session_continues_on_intensity_error() {
        let mut toy = FailingToy;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut mapper = RangeMapper::new(test_mapping_config());
        let running = Arc::new(AtomicBool::new(true));

        // Session should log the error and continue, not bail
        tx.send(ble::BleEvent::RangeUpdate(100)).unwrap();
        tx.send(ble::BleEvent::RangeUpdate(200)).unwrap();
        drop(tx); // channel close triggers disconnect exit

        run_session_inner(&mut toy, &mut rx, &mut mapper, &running)
            .await
            .unwrap();
    }

    // --- Args tests ---

    #[test]
    fn test_args_defaults() {
        let args = Args::try_parse_from(["fancypants"]).unwrap();
        assert_eq!(args.config, PathBuf::from("config.toml"));
        assert!(!args.generate_config);
        assert_eq!(args.log_level, "info");
    }

    #[test]
    fn test_args_custom_config() {
        let args = Args::try_parse_from(["fancypants", "-c", "custom.toml"]).unwrap();
        assert_eq!(args.config, PathBuf::from("custom.toml"));
    }

    #[test]
    fn test_args_generate_config() {
        let args = Args::try_parse_from(["fancypants", "--generate-config"]).unwrap();
        assert!(args.generate_config);
    }

    #[test]
    fn test_args_log_level() {
        let args = Args::try_parse_from(["fancypants", "-l", "debug"]).unwrap();
        assert_eq!(args.log_level, "debug");
    }
}
