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
    /// [default: ./config.toml, then $XDG_CONFIG_HOME/fancypants/config.toml]
    #[arg(short, long, verbatim_doc_comment)]
    config: Option<PathBuf>,

    /// Generate a default config file and exit
    /// (written to -c PATH, or ~/.config/fancypants/config.toml)
    #[arg(long, verbatim_doc_comment)]
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
        let path = generate_config(args.config.as_deref())?;
        info!("Default config written to {:?}", path);
        return Ok(());
    }

    // Load config
    let config = load_config(args.config.as_deref())?;
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

/// Load configuration. An explicitly given path must exist; otherwise search
/// `./config.toml`, then the XDG config dir. If no config exists anywhere,
/// write a default one to the XDG location and use it.
pub(crate) fn load_config(explicit: Option<&Path>) -> anyhow::Result<Config> {
    match explicit {
        Some(path) => Config::load(path)
            .map_err(|e| anyhow::anyhow!("failed to load config {:?}: {:#}", path, e)),
        None => load_or_init_config(Path::new("config.toml"), xdg_config_path()),
    }
}

fn load_or_init_config(local: &Path, xdg: Option<PathBuf>) -> anyhow::Result<Config> {
    if local.exists() {
        info!("Using config {:?}", local);
        return Config::load(local);
    }
    let Some(xdg) = xdg else {
        warn!("No config found and cannot determine config dir (HOME not set); using defaults");
        return Ok(Config::default());
    };
    if xdg.exists() {
        info!("Using config {:?}", xdg);
        return Config::load(&xdg);
    }
    match write_default_config(&xdg) {
        Ok(()) => println!(
            "No config file found — wrote a default one to {} (edit it to customize)",
            xdg.display()
        ),
        Err(e) => warn!(
            "No config found; writing default to {:?} failed: {:#}; using built-in defaults",
            xdg, e
        ),
    }
    Ok(Config::default())
}

/// Write a default config to `explicit`, or to the XDG config dir.
/// Refuses to overwrite an existing file.
pub(crate) fn generate_config(explicit: Option<&Path>) -> anyhow::Result<PathBuf> {
    let path = match explicit {
        Some(p) => p.to_path_buf(),
        None => xdg_config_path().ok_or_else(|| {
            anyhow::anyhow!("cannot determine config dir (HOME not set); pass -c PATH")
        })?,
    };
    if path.exists() {
        anyhow::bail!(
            "{:?} already exists, refusing to overwrite (delete it first or pass -c PATH)",
            path
        );
    }
    write_default_config(&path)?;
    Ok(path)
}

fn write_default_config(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Config::save_default(path)
}

/// Default per-user config location: `$XDG_CONFIG_HOME/fancypants/config.toml`,
/// falling back to `~/.config/fancypants/config.toml`.
fn xdg_config_path() -> Option<PathBuf> {
    config_home(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
    .map(|dir| dir.join("fancypants").join("config.toml"))
}

fn config_home(
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if let Some(xdg) = xdg.filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(xdg));
    }
    home.filter(|v| !v.is_empty())
        .map(|h| PathBuf::from(h).join(".config"))
}

/// Log the loaded configuration summary.
pub(crate) fn log_config(config: &Config) {
    info!("Configuration loaded:");
    info!("  BLE device: {}", config.ble.device_name);
    match config.mapping.mode {
        crate::config::MappingMode::Distance => info!(
            "  Mapping: range [{}-{}mm] -> intensity [{}-{}], invert={}, deadzone={}mm",
            config.mapping.min_range_mm,
            config.mapping.max_range_mm,
            config.mapping.min_intensity,
            config.mapping.max_intensity,
            config.mapping.invert,
            config.mapping.deadzone_mm,
        ),
        crate::config::MappingMode::Velocity => info!(
            "  Mapping: speed [0-{}mm/s] -> intensity [{}-{}], deadzone={}mm",
            config.mapping.max_speed_mm_s,
            config.mapping.min_intensity,
            config.mapping.max_intensity,
            config.mapping.deadzone_mm,
        ),
    }
    info!("  Buttplug server: {}", config.buttplug.server_address);
    for actuator in &config.buttplug.actuator_types {
        let s = actuator.settings();
        info!(
            "  Actuator {}: intensity [{}-{}], pattern={}",
            s.actuator_type, s.min_intensity, s.max_intensity, s.pattern,
        );
    }
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
    toy.find_device(
        config.buttplug.device_index,
        &config.buttplug.actuator_types,
    )
    .await?;

    // 3. Set up range mapper
    let mut mapper = RangeMapper::new(config.mapping.clone());

    // 4. Start BLE notification listener
    let (tx, mut rx) = mpsc::unbounded_channel();
    let ble_handle = {
        let peripheral = peripheral.clone();
        tokio::spawn(async move {
            if let Err(e) = ble::run_ble_client(&peripheral, tx).await {
                error!("BLE client error: {:#}", e);
            }
        })
    };

    let backend: &mut dyn toy::ToyBackend = &mut toy;
    let result = run_session_inner(
        backend,
        &mut rx,
        &mut mapper,
        running,
        std::time::Duration::from_secs(config.ble.data_timeout_secs),
    )
    .await;

    // Cleanup
    info!("Stopping device...");
    let _ = backend.stop().await;
    let _ = backend.disconnect().await;
    ble_handle.abort();

    result
}

/// Core event loop, extracted for testability.
///
/// Returns Err when the session ends because a connection was lost (BLE
/// disconnect, stalled range data, or Intiface dropping), so the caller's
/// reconnect loop retries. Returns Ok only on a requested shutdown.
pub(crate) async fn run_session_inner(
    toy: &mut dyn toy::ToyBackend,
    rx: &mut mpsc::UnboundedReceiver<ble::BleEvent>,
    mapper: &mut RangeMapper,
    running: &Arc<AtomicBool>,
    data_timeout: std::time::Duration,
) -> anyhow::Result<()> {
    info!("Running — move your hand near the sensor!");

    let mut check = tokio::time::interval(std::time::Duration::from_secs(1));
    check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_data = tokio::time::Instant::now();

    while running.load(Ordering::SeqCst) {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(ble::BleEvent::RangeUpdate(distance_mm)) => {
                        last_data = tokio::time::Instant::now();
                        let intensity = mapper.map(distance_mm);
                        if let Err(e) = toy.set_intensity(intensity).await {
                            warn!("Failed to set intensity: {:#}", e);
                        }
                    }
                    Some(ble::BleEvent::Disconnected) | None => {
                        anyhow::bail!("BLE disconnected");
                    }
                    Some(ble::BleEvent::Connected) => {
                        info!("BLE connected");
                    }
                }
            }
            _ = check.tick() => {
                // Periodic check that everything is still alive
                if !toy.is_connected() {
                    anyhow::bail!("Lost connection to Intiface");
                }
                if !data_timeout.is_zero() && last_data.elapsed() >= data_timeout {
                    anyhow::bail!(
                        "No range data for {}s, treating as disconnect",
                        data_timeout.as_secs()
                    );
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

    const TEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

    fn test_mapping_config() -> MappingConfig {
        MappingConfig {
            mode: crate::config::MappingMode::Distance,
            invert: true,
            max_speed_mm_s: 500.0,
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

        let result =
            run_session_inner(&mut toy, &mut rx, &mut mapper, &running, TEST_TIMEOUT).await;

        // Channel close counts as a BLE disconnect, so the session errors to trigger reconnect
        assert!(result.is_err());
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

        let result =
            run_session_inner(&mut toy, &mut rx, &mut mapper, &running, TEST_TIMEOUT).await;

        assert!(result.unwrap_err().to_string().contains("BLE disconnected"));
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

        let result =
            run_session_inner(&mut toy, &mut rx, &mut mapper, &running, TEST_TIMEOUT).await;

        assert!(result.is_err());
        assert_eq!(toy.intensities.len(), 1);
    }

    #[tokio::test]
    async fn test_session_stops_on_running_false() {
        let mut toy = MockToy::new();
        let (_tx, mut rx) = mpsc::unbounded_channel();
        let mut mapper = RangeMapper::new(test_mapping_config());
        let running = Arc::new(AtomicBool::new(false));

        run_session_inner(&mut toy, &mut rx, &mut mapper, &running, TEST_TIMEOUT)
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

        let result =
            run_session_inner(&mut toy, &mut rx, &mut mapper, &running, TEST_TIMEOUT).await;

        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Lost connection to Intiface"));
        assert!(toy.intensities.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_session_stops_when_range_data_stalls() {
        let mut toy = MockToy::new();
        // Keep a sender alive so the channel never closes; data just stops arriving
        let (_tx, mut rx) = mpsc::unbounded_channel();
        let mut mapper = RangeMapper::new(test_mapping_config());
        let running = Arc::new(AtomicBool::new(true));

        let result = run_session_inner(
            &mut toy,
            &mut rx,
            &mut mapper,
            &running,
            std::time::Duration::from_millis(10),
        )
        .await;

        assert!(result.unwrap_err().to_string().contains("No range data"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_session_zero_timeout_disables_watchdog() {
        let mut toy = MockToy::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut mapper = RangeMapper::new(test_mapping_config());
        let running = Arc::new(AtomicBool::new(true));

        tx.send(ble::BleEvent::RangeUpdate(165)).unwrap();
        drop(tx);

        let result = run_session_inner(
            &mut toy,
            &mut rx,
            &mut mapper,
            &running,
            std::time::Duration::ZERO,
        )
        .await;

        // Exits via channel close (disconnect), not the watchdog
        assert!(result.unwrap_err().to_string().contains("BLE disconnected"));
        assert_eq!(toy.intensities.len(), 1);
    }

    // --- load_config / generate_config tests ---

    #[test]
    fn test_load_config_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        Config::save_default(&path).unwrap();
        let config = load_config(Some(&path)).unwrap();
        assert_eq!(config.ble.device_name, "Fancypants");
    }

    #[test]
    fn test_load_config_explicit_missing_is_error() {
        let err = load_config(Some(Path::new("/tmp/nonexistent_fp_config.toml"))).unwrap_err();
        assert!(
            err.to_string().contains("nonexistent_fp_config.toml"),
            "error should name the missing file: {err}"
        );
    }

    #[test]
    fn test_generate_config_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let written = generate_config(Some(&path)).unwrap();
        assert_eq!(written, path);
        let config = load_config(Some(&path)).unwrap();
        assert_eq!(config.ble.device_name, "Fancypants");
    }

    #[test]
    fn test_generate_config_refuses_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        generate_config(Some(&path)).unwrap();
        let err = generate_config(Some(&path)).unwrap_err();
        assert!(
            err.to_string().contains("refusing to overwrite"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_load_or_init_auto_generates_xdg_config() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("absent.toml");
        let xdg = dir.path().join("nested").join("config.toml");

        let config = load_or_init_config(&local, Some(xdg.clone())).unwrap();
        assert_eq!(config.ble.device_name, "Fancypants");
        assert!(xdg.exists(), "default config should have been written");
        // The generated file must load cleanly on the next run
        let reloaded = load_config(Some(&xdg)).unwrap();
        assert_eq!(reloaded.ble.device_name, "Fancypants");
    }

    #[test]
    fn test_load_or_init_prefers_local() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("config.toml");
        Config::save_default(&local).unwrap();
        let xdg = dir.path().join("xdg").join("config.toml");

        load_or_init_config(&local, Some(xdg.clone())).unwrap();
        assert!(
            !xdg.exists(),
            "should not generate when a local config exists"
        );
    }

    #[test]
    fn test_load_or_init_uses_existing_xdg_config() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("absent.toml");
        let xdg = dir.path().join("config.toml");
        let toml = toml::to_string_pretty(&Config::default())
            .unwrap()
            .replace("device_name = \"Fancypants\"", "device_name = \"Custom\"");
        std::fs::write(&xdg, toml).unwrap();

        let config = load_or_init_config(&local, Some(xdg)).unwrap();
        assert_eq!(config.ble.device_name, "Custom");
    }

    #[test]
    fn test_load_or_init_write_failure_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("absent.toml");
        // Parent of the target path is a file, so writing must fail
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "").unwrap();
        let xdg = blocker.join("config.toml");

        let config = load_or_init_config(&local, Some(xdg)).unwrap();
        assert_eq!(config.ble.device_name, "Fancypants");
    }

    #[test]
    fn test_load_or_init_no_xdg_dir_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("absent.toml");
        let config = load_or_init_config(&local, None).unwrap();
        assert_eq!(config.ble.device_name, "Fancypants");
    }

    #[test]
    fn test_config_home_prefers_xdg() {
        let home = config_home(Some("/xdg".into()), Some("/home/u".into())).unwrap();
        assert_eq!(home, PathBuf::from("/xdg"));
    }

    #[test]
    fn test_config_home_falls_back_to_home() {
        let home = config_home(None, Some("/home/u".into())).unwrap();
        assert_eq!(home, PathBuf::from("/home/u/.config"));
        // Empty XDG_CONFIG_HOME is treated as unset, per the XDG spec
        let home = config_home(Some("".into()), Some("/home/u".into())).unwrap();
        assert_eq!(home, PathBuf::from("/home/u/.config"));
    }

    #[test]
    fn test_config_home_none_without_home() {
        assert!(config_home(None, None).is_none());
        assert!(config_home(Some("".into()), Some("".into())).is_none());
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

        let result =
            run_session_inner(&mut toy, &mut rx, &mut mapper, &running, TEST_TIMEOUT).await;

        // Errors only from the disconnect, after both updates were attempted
        assert!(result.unwrap_err().to_string().contains("BLE disconnected"));
    }

    // --- Args tests ---

    #[test]
    fn test_args_defaults() {
        let args = Args::try_parse_from(["fancypants"]).unwrap();
        assert_eq!(args.config, None);
        assert!(!args.generate_config);
        assert_eq!(args.log_level, "info");
    }

    #[test]
    fn test_args_custom_config() {
        let args = Args::try_parse_from(["fancypants", "-c", "custom.toml"]).unwrap();
        assert_eq!(args.config, Some(PathBuf::from("custom.toml")));
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
