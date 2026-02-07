use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub ble: BleConfig,
    pub mapping: MappingConfig,
    pub buttplug: ButtplugConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BleConfig {
    /// BLE device name to scan for (must match CONFIG_BT_DEVICE_NAME in firmware)
    pub device_name: String,
    /// Scan timeout in seconds
    pub scan_timeout_secs: u64,
    /// Reconnect delay on disconnect
    pub reconnect_delay_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingConfig {
    /// Invert the mapping: closer = more intense (true) or further = more intense (false)
    pub invert: bool,
    /// Minimum range in mm (sensor readings below this map to max/min intensity)
    pub min_range_mm: u16,
    /// Maximum range in mm (sensor readings above this map to min/max intensity)
    pub max_range_mm: u16,
    /// Minimum output intensity (0.0 - 1.0)
    pub min_intensity: f64,
    /// Maximum output intensity (0.0 - 1.0)
    pub max_intensity: f64,
    /// Dead zone: distances above this produce zero intensity (0 = disabled)
    pub deadzone_mm: u16,
    /// Smoothing: exponential moving average factor (0.0 = no smoothing, 1.0 = max smoothing)
    pub smoothing: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtplugConfig {
    /// Intiface Engine websocket address
    pub server_address: String,
    /// Device index to control (None = first available)
    pub device_index: Option<u32>,
    /// Which actuator types to control
    pub actuator_types: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            ble: BleConfig {
                device_name: "Rangefinder".to_string(),
                scan_timeout_secs: 30,
                reconnect_delay_secs: 5,
            },
            mapping: MappingConfig {
                invert: true,  // closer = more intense
                min_range_mm: 30,
                max_range_mm: 300,
                min_intensity: 0.0,
                max_intensity: 1.0,
                deadzone_mm: 500,
                smoothing: 0.3,
            },
            buttplug: ButtplugConfig {
                server_address: "ws://127.0.0.1:12345".to_string(),
                device_index: None,
                actuator_types: vec!["Vibrate".to_string()],
            },
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn save_default(path: &Path) -> anyhow::Result<()> {
        let config = Config::default();
        let content = toml::to_string_pretty(&config)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    fn validate(&self) -> anyhow::Result<()> {
        if self.mapping.min_intensity < 0.0 || self.mapping.min_intensity > 1.0 {
            anyhow::bail!("min_intensity must be 0.0-1.0");
        }
        if self.mapping.max_intensity < 0.0 || self.mapping.max_intensity > 1.0 {
            anyhow::bail!("max_intensity must be 0.0-1.0");
        }
        if self.mapping.min_range_mm >= self.mapping.max_range_mm {
            anyhow::bail!("min_range_mm must be < max_range_mm");
        }
        if self.mapping.smoothing < 0.0 || self.mapping.smoothing > 1.0 {
            anyhow::bail!("smoothing must be 0.0-1.0");
        }
        Ok(())
    }
}
