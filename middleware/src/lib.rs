pub mod ble;
pub mod config;
pub mod mapper;
pub mod toy;

use btleplug::api::Peripheral as _;
pub use config::Config;
pub use mapper::RangeMapper;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
pub use toy::ToyTelemetry;
use tracing::{error, info, warn};

/// High-level session lifecycle state emitted in `SessionTelemetry`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Stopped,
    ScanningSensor,
    ConnectingToys,
    Running,
    Reconnecting,
    Error,
}

/// Real-time telemetry snapshot emitted by the session runner for UI dashboards.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionTelemetry {
    pub state: SessionState,
    pub message: String,
    pub distance_mm: Option<u16>,
    pub mapped_intensity: f64,
    pub battery_pct: Option<u8>,
    pub toys: Vec<ToyTelemetry>,
}

impl Default for SessionTelemetry {
    fn default() -> Self {
        SessionTelemetry {
            state: SessionState::Stopped,
            message: "Idle".to_string(),
            distance_mm: None,
            mapped_intensity: 0.0,
            battery_pct: None,
            toys: Vec::new(),
        }
    }
}

/// Handle to a running background session (used by the Tauri GUI / mobile app).
pub struct SessionHandle {
    running: Arc<AtomicBool>,
    config_tx: watch::Sender<Config>,
    task: tokio::task::JoinHandle<()>,
}

impl SessionHandle {
    /// Spawn a background session loop with live telemetry and hot-reloadable config.
    pub fn spawn<F>(initial_config: Config, on_telemetry: F) -> Self
    where
        F: Fn(SessionTelemetry) + Send + Sync + 'static,
    {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let (config_tx, mut config_rx) = watch::channel(initial_config.clone());
        let callback: Arc<dyn Fn(SessionTelemetry) + Send + Sync> = Arc::new(on_telemetry);

        let task = tokio::spawn(async move {
            while running_clone.load(Ordering::SeqCst) {
                let current_cfg = config_rx.borrow().clone();
                match run_managed_session(&current_cfg, &running_clone, &mut config_rx, &callback)
                    .await
                {
                    Ok(()) => {
                        callback(SessionTelemetry {
                            state: SessionState::Stopped,
                            message: "Session stopped".to_string(),
                            ..Default::default()
                        });
                        break;
                    }
                    Err(e) => {
                        error!("Session error: {:#}", e);
                        if !running_clone.load(Ordering::SeqCst) {
                            callback(SessionTelemetry {
                                state: SessionState::Stopped,
                                message: "Session stopped".to_string(),
                                ..Default::default()
                            });
                            break;
                        }
                        let delay = config_rx.borrow().ble.reconnect_delay_secs;
                        callback(SessionTelemetry {
                            state: SessionState::Reconnecting,
                            message: format!("{:#} — reconnecting in {}s...", e, delay),
                            ..Default::default()
                        });
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    }
                }
            }
        });

        SessionHandle {
            running,
            config_tx,
            task,
        }
    }

    /// Hot-reload mapping and actuator settings without disconnecting.
    pub fn update_config(&self, config: Config) -> anyhow::Result<()> {
        config.validate()?;
        let _ = self.config_tx.send(config);
        Ok(())
    }

    /// Request graceful shutdown and wait for device cleanup.
    pub async fn stop(self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), self.task).await;
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst) && !self.task.is_finished()
    }
}

async fn run_managed_session(
    config: &Config,
    running: &Arc<AtomicBool>,
    config_rx: &mut watch::Receiver<Config>,
    on_telemetry: &Arc<dyn Fn(SessionTelemetry) + Send + Sync>,
) -> anyhow::Result<()> {
    on_telemetry(SessionTelemetry {
        state: SessionState::ScanningSensor,
        message: format!("Scanning BLE for '{}'...", config.ble.device_name),
        ..Default::default()
    });

    let peripheral =
        ble::find_device(&config.ble.device_name, config.ble.scan_timeout_secs).await?;

    on_telemetry(SessionTelemetry {
        state: SessionState::ConnectingToys,
        message: match config.buttplug.mode {
            config::EngineMode::Intiface => {
                format!(
                    "Connecting to Intiface ({})...",
                    config.buttplug.server_address
                )
            }
            config::EngineMode::Standalone => {
                "Scanning for BLE toys (standalone mode)...".to_string()
            }
        },
        ..Default::default()
    });

    let mut toy = toy::ToyController::connect_for_config(&config.buttplug).await?;
    let targets = config.resolved_device_targets();
    toy.find_devices(&targets).await?;

    let mut mapper = RangeMapper::new(config.mapping.clone());

    let (tx, mut rx) = mpsc::unbounded_channel();
    let ble_handle = {
        let peripheral = peripheral.clone();
        let sample_interval_ms = config.ble.sample_interval_ms;
        let notify_interval_ms = config.ble.notify_interval_ms;
        tokio::spawn(async move {
            if let Err(e) =
                ble::run_ble_client(&peripheral, sample_interval_ms, notify_interval_ms, tx).await
            {
                error!("BLE client error: {:#}", e);
            }
        })
    };

    let backend: &mut dyn toy::ToyBackend = &mut toy;
    let result = run_session_with_telemetry(
        backend,
        &mut rx,
        &mut mapper,
        running,
        std::time::Duration::from_secs(config.ble.data_timeout_secs),
        Some(config_rx),
        Some(on_telemetry),
    )
    .await;

    info!("Stopping device...");
    let _ = backend.stop().await;
    let _ = backend.disconnect().await;
    ble_handle.abort();
    let _ = peripheral.disconnect().await;

    result
}

/// Load configuration. An explicitly given path must exist; otherwise search
/// `./config.toml`, then the XDG config dir. If no config exists anywhere,
/// write a default one to the XDG location and use it.
pub fn load_config(explicit: Option<&Path>) -> anyhow::Result<Config> {
    match explicit {
        Some(path) => Config::load(path)
            .map_err(|e| anyhow::anyhow!("failed to load config {:?}: {:#}", path, e)),
        None => load_or_init_config(Path::new("config.toml"), xdg_config_path()),
    }
}

pub fn load_or_init_config(local: &Path, xdg: Option<PathBuf>) -> anyhow::Result<Config> {
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
pub fn generate_config(explicit: Option<&Path>) -> anyhow::Result<PathBuf> {
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
pub fn xdg_config_path() -> Option<PathBuf> {
    config_home(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
    .map(|dir| dir.join("fancypants").join("config.toml"))
}

pub fn config_home(
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
pub fn log_config(config: &Config) {
    info!("Configuration loaded:");
    info!("  BLE device: {}", config.ble.device_name);
    log_mapping("  Default mapping", &config.mapping);
    info!(
        "  Buttplug mode: {} (server: {})",
        config.buttplug.mode, config.buttplug.server_address
    );
    for (idx, target) in config.resolved_device_targets().iter().enumerate() {
        info!(
            "  Target {}: index={:?}, name={:?}, mode={}",
            idx, target.device_index, target.device_name, target.mapping.mode
        );
        for actuator in &target.actuator_types {
            let s = actuator.settings();
            info!(
                "    Actuator {}: intensity [{}-{}], pattern={}",
                s.actuator_type, s.min_intensity, s.max_intensity, s.pattern,
            );
        }
    }
}

fn log_mapping(prefix: &str, mapping: &crate::config::MappingConfig) {
    match mapping.mode {
        crate::config::MappingMode::Distance => info!(
            "{}: range [{}-{}mm] -> intensity [{}-{}], invert={}, deadzone={}mm",
            prefix,
            mapping.min_range_mm,
            mapping.max_range_mm,
            mapping.min_intensity,
            mapping.max_intensity,
            mapping.invert,
            mapping.deadzone_mm,
        ),
        crate::config::MappingMode::Velocity => info!(
            "{}: speed [0-{}mm/s] -> intensity [{}-{}], deadzone={}mm",
            prefix,
            mapping.max_speed_mm_s,
            mapping.min_intensity,
            mapping.max_intensity,
            mapping.deadzone_mm,
        ),
    }
}

/// Reconnect loop: runs sessions until clean exit or shutdown signal.
pub async fn reconnect_loop(
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
pub trait AsyncSessionFn {
    async fn run(&self, config: &Config, running: &Arc<AtomicBool>) -> anyhow::Result<()>;
}

pub struct RealSession;

#[async_trait::async_trait]
impl AsyncSessionFn for RealSession {
    async fn run(&self, config: &Config, running: &Arc<AtomicBool>) -> anyhow::Result<()> {
        run_session(config, running).await
    }
}

pub async fn run_session(config: &Config, running: &Arc<AtomicBool>) -> anyhow::Result<()> {
    // 1. Find fancypants-nrf52 BLE device
    let peripheral: btleplug::platform::Peripheral =
        ble::find_device(&config.ble.device_name, config.ble.scan_timeout_secs).await?;

    // 2. Connect to Intiface Engine or standalone embedded server and configure target toys
    let mut toy: toy::ToyController =
        toy::ToyController::connect_for_config(&config.buttplug).await?;
    let targets = config.resolved_device_targets();
    toy.find_devices(&targets).await?;

    // 3. Set up range mapper
    let mut mapper = RangeMapper::new(config.mapping.clone());

    // 4. Start BLE notification listener
    let (tx, mut rx) = mpsc::unbounded_channel();
    let ble_handle = {
        let peripheral = peripheral.clone();
        let sample_interval_ms = config.ble.sample_interval_ms;
        let notify_interval_ms = config.ble.notify_interval_ms;
        tokio::spawn(async move {
            if let Err(e) =
                ble::run_ble_client(&peripheral, sample_interval_ms, notify_interval_ms, tx).await
            {
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
    let _ = peripheral.disconnect().await;

    result
}

/// Core event loop, extracted for testability.
pub async fn run_session_inner(
    toy: &mut dyn toy::ToyBackend,
    rx: &mut mpsc::UnboundedReceiver<ble::BleEvent>,
    mapper: &mut RangeMapper,
    running: &Arc<AtomicBool>,
    data_timeout: std::time::Duration,
) -> anyhow::Result<()> {
    run_session_with_telemetry(toy, rx, mapper, running, data_timeout, None, None).await
}

/// Core event loop with optional live config hot-reload and telemetry emission.
pub async fn run_session_with_telemetry(
    toy: &mut dyn toy::ToyBackend,
    rx: &mut mpsc::UnboundedReceiver<ble::BleEvent>,
    mapper: &mut RangeMapper,
    running: &Arc<AtomicBool>,
    data_timeout: std::time::Duration,
    mut config_rx: Option<&mut watch::Receiver<Config>>,
    on_telemetry: Option<&Arc<dyn Fn(SessionTelemetry) + Send + Sync>>,
) -> anyhow::Result<()> {
    info!("Running — move your hand near the sensor!");

    let mut last_distance_mm: Option<u16> = None;
    let mut last_intensity: f64 = 0.0;
    let mut last_battery_pct: Option<u8> = None;

    if let Some(cb) = on_telemetry {
        cb(SessionTelemetry {
            state: SessionState::Running,
            message: "Running — move your hand near the sensor!".to_string(),
            distance_mm: None,
            mapped_intensity: 0.0,
            battery_pct: None,
            toys: toy.telemetry(),
        });
    }

    let mut check = tokio::time::interval(std::time::Duration::from_secs(1));
    check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut pulse_tick = tokio::time::interval(std::time::Duration::from_millis(25));
    pulse_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_data = tokio::time::Instant::now();

    while running.load(Ordering::SeqCst) {
        if let Some(rx_cfg) = config_rx.as_mut() {
            if rx_cfg.has_changed().unwrap_or(false) {
                let updated = rx_cfg.borrow_and_update().clone();
                mapper.update_config(updated.mapping.clone());
                let _ = toy.apply_targets(&updated.resolved_device_targets());
            }
        }
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(ble::BleEvent::RangeUpdate(distance_mm)) => {
                        last_data = tokio::time::Instant::now();
                        last_distance_mm = Some(distance_mm);
                        let intensity = mapper.map(distance_mm);
                        last_intensity = intensity;
                        match toy.update_range(distance_mm).await {
                            Ok(true) => {}
                            Ok(false) => {
                                if let Err(e) = toy.set_intensity(intensity).await {
                                    warn!("Failed to set intensity: {:#}", e);
                                }
                            }
                            Err(e) => {
                                warn!("Failed to set intensity: {:#}", e);
                            }
                        }
                        if let Some(cb) = on_telemetry {
                            cb(SessionTelemetry {
                                state: SessionState::Running,
                                message: "Running".to_string(),
                                distance_mm: last_distance_mm,
                                mapped_intensity: last_intensity,
                                battery_pct: last_battery_pct,
                                toys: toy.telemetry(),
                            });
                        }
                    }
                    Some(ble::BleEvent::BatteryUpdate(pct)) => {
                        last_battery_pct = Some(pct);
                        if pct <= 20 {
                            warn!("Sensor battery low: {}%", pct);
                        } else {
                            info!("Sensor battery: {}%", pct);
                        }
                        if let Some(cb) = on_telemetry {
                            cb(SessionTelemetry {
                                state: SessionState::Running,
                                message: "Running".to_string(),
                                distance_mm: last_distance_mm,
                                mapped_intensity: last_intensity,
                                battery_pct: last_battery_pct,
                                toys: toy.telemetry(),
                            });
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
            _ = pulse_tick.tick() => {
                if let Err(e) = toy.tick().await {
                    warn!("Failed to update pulse pattern: {:#}", e);
                }
            }
            _ = check.tick() => {
                if let Some(rx_cfg) = config_rx.as_mut() {
                    if rx_cfg.has_changed().unwrap_or(false) {
                        let updated = rx_cfg.borrow_and_update().clone();
                        mapper.update_config(updated.mapping.clone());
                        let _ = toy.apply_targets(&updated.resolved_device_targets());
                    }
                }
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
    use std::sync::Mutex;

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

    // --- run_session_inner / telemetry tests ---

    #[tokio::test]
    async fn test_session_processes_range_updates_and_emits_telemetry() {
        let mut toy = MockToy::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut mapper = RangeMapper::new(test_mapping_config());
        let running = Arc::new(AtomicBool::new(true));
        let events: Arc<Mutex<Vec<SessionTelemetry>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let cb: Arc<dyn Fn(SessionTelemetry) + Send + Sync> = Arc::new(move |t| {
            events_clone.lock().unwrap().push(t);
        });

        tx.send(ble::BleEvent::BatteryUpdate(88)).unwrap();
        tx.send(ble::BleEvent::RangeUpdate(30)).unwrap();
        tx.send(ble::BleEvent::RangeUpdate(300)).unwrap();
        drop(tx);

        let result = run_session_with_telemetry(
            &mut toy,
            &mut rx,
            &mut mapper,
            &running,
            TEST_TIMEOUT,
            None,
            Some(&cb),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(toy.intensities.len(), 2);
        assert!((toy.intensities[0] - 1.0).abs() < 0.01);
        assert!((toy.intensities[1] - 0.0).abs() < 0.01);

        let recorded = events.lock().unwrap();
        assert!(recorded.len() >= 3);
        assert_eq!(recorded.last().unwrap().distance_mm, Some(300));
        assert_eq!(recorded.last().unwrap().battery_pct, Some(88));
    }

    #[tokio::test]
    async fn test_session_hot_reloads_config_on_watch_change() {
        let mut toy = MockToy::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut mapper = RangeMapper::new(test_mapping_config());
        let running = Arc::new(AtomicBool::new(true));

        let mut updated_cfg = Config::default();
        updated_cfg.mapping.invert = false;
        let (cfg_tx, mut cfg_rx) = watch::channel(Config::default());
        cfg_tx.send(updated_cfg).unwrap();

        tx.send(ble::BleEvent::RangeUpdate(30)).unwrap();
        drop(tx);

        let result = run_session_with_telemetry(
            &mut toy,
            &mut rx,
            &mut mapper,
            &running,
            TEST_TIMEOUT,
            Some(&mut cfg_rx),
            None,
        )
        .await;

        assert!(result.is_err());
        // With invert=false hot-reloaded, 30mm (closest) maps to ~0.0
        assert_eq!(toy.intensities.len(), 1);
        assert!((toy.intensities[0] - 0.0).abs() < 0.01);
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
        tx.send(ble::BleEvent::BatteryUpdate(85)).unwrap();
        tx.send(ble::BleEvent::BatteryUpdate(15)).unwrap();
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
        let home = config_home(Some("".into()), Some("/home/u".into())).unwrap();
        assert_eq!(home, PathBuf::from("/home/u/.config"));
    }

    #[test]
    fn test_config_home_none_without_home() {
        assert!(config_home(None, None).is_none());
        assert!(config_home(Some("".into()), Some("".into())).is_none());
    }

    #[test]
    fn test_log_config_does_not_panic() {
        let config = Config::default();
        log_config(&config);
    }

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

        tx.send(ble::BleEvent::RangeUpdate(100)).unwrap();
        tx.send(ble::BleEvent::RangeUpdate(200)).unwrap();
        drop(tx);

        let result =
            run_session_inner(&mut toy, &mut rx, &mut mapper, &running, TEST_TIMEOUT).await;

        assert!(result.unwrap_err().to_string().contains("BLE disconnected"));
    }
}
