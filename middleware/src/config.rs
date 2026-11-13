use buttplug::core::message::ActuatorType;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub ble: BleConfig,
    pub mapping: MappingConfig,
    pub buttplug: ButtplugConfig,
}

/// Commented default configuration template written on first run or `--generate-config`.
pub const DEFAULT_CONFIG_TOML: &str = r#"# Fancypants Configuration
# =====================================

[ble]
# Must match CONFIG_BT_DEVICE_NAME in firmware prj.conf
device_name = "Fancypants"
# How long to scan before giving up (seconds)
scan_timeout_secs = 30
# Delay before reconnecting after disconnect (seconds)
reconnect_delay_secs = 5
# Failsafe: stop the toy and reconnect if no range data arrives
# for this long (seconds, 0 = disabled)
data_timeout_secs = 5
# Optional: override the firmware's sensor sampling and BLE notification
# intervals in milliseconds (10-5000, firmware defaults to 50ms = 20Hz)
# sample_interval_ms = 50
# notify_interval_ms = 50

[mapping]
# What drives the intensity:
#   "distance" = intensity follows the current distance reading (default)
#   "velocity" = intensity follows how fast the distance is changing;
#                movement in either direction counts, holding still = off
mode = "distance"

# Distance mode only:
# true = closer to sensor means more intense (default, most intuitive)
# false = further from sensor means more intense
invert = true

# Velocity mode only: speed (mm/s) that maps to max intensity.
# Slow hand waving is ~100-300mm/s, fast is ~1000mm/s.
max_speed_mm_s = 500.0

# Sensor range window (millimeters)
# VL53L0X effective range is ~30-1200mm
# These define the "active zone" for intensity mapping
min_range_mm = 30
max_range_mm = 300

# Output intensity range (0.0 - 1.0)
min_intensity = 0.0
max_intensity = 1.0

# Distances beyond this produce zero intensity (0 = disabled)
# Useful to create an "off" zone when you pull away
deadzone_mm = 500

# Exponential moving average smoothing factor
# 0.0 = no smoothing (raw sensor data), 1.0 = maximum smoothing
# 0.2-0.4 feels responsive but eliminates jitter
smoothing = 0.3

[buttplug]
# Engine connection mode:
#   "intiface"   = connect to an external Intiface Central / Engine via WebSocket (default)
#   "standalone" = run an embedded Buttplug server in-process (connects to BLE toys directly)
mode = "intiface"

# Intiface Engine websocket address (used when mode = "intiface")
# Default port for Intiface Central / Intiface Engine
server_address = "ws://127.0.0.1:12345"

# Device index to control (comment out to use first available matching device)
# device_index = 0

# Actuator types to target when `devices` is not specified. Each entry is
# either just a type name:
#   actuator_types = ["Vibrate"]
# or a table with per-actuator settings:
#   actuator_types = [
#       { type = "Vibrate", max_intensity = 0.8 },
#       { type = "Oscillate", pattern = "pulse", pulse_hz = 2.0 },
#   ]
# Types: Vibrate, Rotate, Oscillate, Constrict, Inflate, Position
# min_intensity / max_intensity (0.0-1.0) rescale the mapped intensity for
# that actuator (at mapped intensity 0 the actuator is off).
# pattern is "constant" (follow the sensor, default) or "pulse"
# (on/off square wave at pulse_hz while active).
# If a toy has several actuators of the same type, the nth entry of that type
# configures its nth actuator; extras reuse the last entry.
actuator_types = ["Vibrate"]

# Multi-toy configuration (optional):
# Add one or more `[[buttplug.devices]]` blocks to control multiple toys at
# once, each with its own actuator settings and optional `[buttplug.devices.mapping]`
# override (for example, one toy driven by proximity and another by speed).
# Match toys by `device_index` or substring `device_name` (or omit both to
# claim the next available matching toy).
#
# [[buttplug.devices]]
# device_name = "Lush"
# actuator_types = ["Vibrate"]
#
# [[buttplug.devices]]
# device_name = "Melt"
# actuator_types = [{ type = "Oscillate", max_intensity = 0.7, pattern = "pulse", pulse_hz = 2.0 }]
# [buttplug.devices.mapping]
# mode = "velocity"
# max_speed_mm_s = 600.0
# smoothing = 0.5
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BleConfig {
    /// BLE device name to scan for (must match CONFIG_BT_DEVICE_NAME in firmware)
    pub device_name: String,
    /// Scan timeout in seconds
    pub scan_timeout_secs: u64,
    /// Reconnect delay on disconnect
    pub reconnect_delay_secs: u64,
    /// Treat the sensor as disconnected if no range data arrives for this long
    #[serde(default = "default_data_timeout_secs")]
    pub data_timeout_secs: u64,
    /// Optional sensor sampling interval in ms to write to the firmware config characteristic (10-5000)
    #[serde(default)]
    pub sample_interval_ms: Option<u16>,
    /// Optional BLE notification interval in ms to write to the firmware config characteristic (10-5000)
    #[serde(default)]
    pub notify_interval_ms: Option<u16>,
}

fn default_data_timeout_secs() -> u64 {
    5
}

fn default_max_speed_mm_s() -> f64 {
    500.0
}

/// What the intensity follows: the current distance, or how fast it is changing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MappingMode {
    /// Intensity follows the current distance reading (also accepts `"proximity"`)
    #[default]
    #[serde(alias = "proximity")]
    Distance,
    /// Intensity follows the speed at which the distance is changing (also accepts `"speed"`)
    #[serde(alias = "speed")]
    Velocity,
}

impl std::fmt::Display for MappingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MappingMode::Distance => write!(f, "distance"),
            MappingMode::Velocity => write!(f, "velocity"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MappingConfig {
    /// What drives the intensity: "distance" / "proximity" (current reading) or
    /// "velocity" / "speed" (rate of change of the reading)
    #[serde(default)]
    pub mode: MappingMode,
    /// Invert the mapping: closer = more intense (true) or further = more intense (false).
    /// Only used in distance mode.
    pub invert: bool,
    /// Speed (mm/s) that maps to max intensity in velocity mode
    #[serde(default = "default_max_speed_mm_s")]
    pub max_speed_mm_s: f64,
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

impl MappingConfig {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        if self.min_intensity < 0.0 || self.min_intensity > 1.0 {
            anyhow::bail!("min_intensity must be 0.0-1.0");
        }
        if self.max_intensity < 0.0 || self.max_intensity > 1.0 {
            anyhow::bail!("max_intensity must be 0.0-1.0");
        }
        if self.min_range_mm >= self.max_range_mm {
            anyhow::bail!("min_range_mm must be < max_range_mm");
        }
        if self.smoothing < 0.0 || self.smoothing > 1.0 {
            anyhow::bail!("smoothing must be 0.0-1.0");
        }
        if !self.max_speed_mm_s.is_finite() || self.max_speed_mm_s <= 0.0 {
            anyhow::bail!("max_speed_mm_s must be > 0");
        }
        Ok(())
    }
}

/// Optional per-device mapping overrides. Any field left unset inherits from
/// the top-level `[mapping]` section.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MappingOverride {
    pub mode: Option<MappingMode>,
    pub invert: Option<bool>,
    pub max_speed_mm_s: Option<f64>,
    pub min_range_mm: Option<u16>,
    pub max_range_mm: Option<u16>,
    pub min_intensity: Option<f64>,
    pub max_intensity: Option<f64>,
    pub deadzone_mm: Option<u16>,
    pub smoothing: Option<f64>,
}

impl MappingOverride {
    /// Merge this override onto `base`, returning a complete `MappingConfig`.
    pub fn resolve(&self, base: &MappingConfig) -> MappingConfig {
        MappingConfig {
            mode: self.mode.unwrap_or(base.mode),
            invert: self.invert.unwrap_or(base.invert),
            max_speed_mm_s: self.max_speed_mm_s.unwrap_or(base.max_speed_mm_s),
            min_range_mm: self.min_range_mm.unwrap_or(base.min_range_mm),
            max_range_mm: self.max_range_mm.unwrap_or(base.max_range_mm),
            min_intensity: self.min_intensity.unwrap_or(base.min_intensity),
            max_intensity: self.max_intensity.unwrap_or(base.max_intensity),
            deadzone_mm: self.deadzone_mm.unwrap_or(base.deadzone_mm),
            smoothing: self.smoothing.unwrap_or(base.smoothing),
        }
    }
}

/// Per-toy target configuration (`[[buttplug.devices]]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTargetConfig {
    /// Optional Buttplug device index to match
    pub device_index: Option<u32>,
    /// Optional case-insensitive device name substring to match
    pub device_name: Option<String>,
    /// Actuator types to drive on this device (defaults to `["Vibrate"]`)
    #[serde(default = "default_actuator_types")]
    pub actuator_types: Vec<ActuatorConfig>,
    /// Optional mapping overrides for this specific device
    #[serde(default)]
    pub mapping: Option<MappingOverride>,
}

/// A fully resolved target device configuration with its own `MappingConfig`.
#[derive(Debug, Clone)]
pub struct ResolvedDeviceTarget {
    pub device_index: Option<u32>,
    pub device_name: Option<String>,
    pub actuator_types: Vec<ActuatorConfig>,
    pub mapping: MappingConfig,
}

/// How Fancypants connects to Buttplug toys: via an external Intiface Engine
/// WebSocket server, or via an embedded in-process Buttplug server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineMode {
    /// Connect to an external Intiface Central / Engine over WebSocket
    #[default]
    Intiface,
    /// Run an embedded Buttplug server in-process and connect to BLE toys directly
    #[serde(alias = "embedded")]
    Standalone,
}

impl std::fmt::Display for EngineMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineMode::Intiface => write!(f, "intiface"),
            EngineMode::Standalone => write!(f, "standalone"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtplugConfig {
    /// Whether to connect to Intiface Central ("intiface") or run an embedded
    /// Buttplug server ("standalone")
    #[serde(default)]
    pub mode: EngineMode,
    /// Intiface Engine websocket address (used when mode = "intiface")
    #[serde(default = "default_server_address")]
    pub server_address: String,
    /// Device index to control when `devices` is empty (None = first available)
    pub device_index: Option<u32>,
    /// Actuator types to target when `devices` is empty, with optional per-actuator settings
    #[serde(default = "default_actuator_types")]
    pub actuator_types: Vec<ActuatorConfig>,
    /// Optional list of per-toy configurations for controlling multiple toys independently
    #[serde(default)]
    pub devices: Vec<DeviceTargetConfig>,
}

fn default_server_address() -> String {
    "ws://127.0.0.1:12345".to_string()
}

fn default_actuator_types() -> Vec<ActuatorConfig> {
    vec![ActuatorConfig::Type("Vibrate".to_string())]
}

/// One entry in `actuator_types`: either a plain type name (`"Vibrate"`) or a
/// table with per-actuator settings
/// (`{ type = "Vibrate", max_intensity = 0.8, pattern = "pulse" }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActuatorConfig {
    Type(String),
    Settings(ActuatorSettings),
}

impl ActuatorConfig {
    /// Normalize to full settings, filling in defaults for the shorthand form.
    pub fn settings(&self) -> ActuatorSettings {
        match self {
            ActuatorConfig::Type(name) => ActuatorSettings {
                actuator_type: name.clone(),
                min_intensity: 0.0,
                max_intensity: 1.0,
                pattern: Pattern::Constant,
                pulse_hz: default_pulse_hz(),
            },
            ActuatorConfig::Settings(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActuatorSettings {
    /// Actuator type: Vibrate, Rotate, Oscillate, Constrict, Inflate, Position
    #[serde(rename = "type")]
    pub actuator_type: String,
    /// Intensity this actuator runs at when the mapped intensity is just above
    /// zero (0.0 - 1.0); at mapped intensity 0.0 the actuator is off
    #[serde(default)]
    pub min_intensity: f64,
    /// Intensity this actuator runs at when the mapped intensity is 1.0
    #[serde(default = "default_max_intensity")]
    pub max_intensity: f64,
    /// How the actuator follows the mapped intensity
    #[serde(default)]
    pub pattern: Pattern,
    /// Pulse frequency in Hz (only used with pattern = "pulse")
    #[serde(default = "default_pulse_hz")]
    pub pulse_hz: f64,
}

fn default_max_intensity() -> f64 {
    1.0
}

fn default_pulse_hz() -> f64 {
    1.0
}

impl ActuatorSettings {
    pub fn parsed_type(&self) -> anyhow::Result<ActuatorType> {
        match self.actuator_type.to_ascii_lowercase().as_str() {
            "vibrate" => Ok(ActuatorType::Vibrate),
            "rotate" => Ok(ActuatorType::Rotate),
            "oscillate" => Ok(ActuatorType::Oscillate),
            "constrict" => Ok(ActuatorType::Constrict),
            "inflate" => Ok(ActuatorType::Inflate),
            "position" => Ok(ActuatorType::Position),
            other => anyhow::bail!(
                "unknown actuator type {:?} (expected one of: Vibrate, Rotate, \
                 Oscillate, Constrict, Inflate, Position)",
                other
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pattern {
    /// Follow the mapped intensity directly
    #[default]
    Constant,
    /// Square-wave on/off at `pulse_hz` while the mapped intensity is above zero
    Pulse,
}

impl std::fmt::Display for Pattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Pattern::Constant => write!(f, "constant"),
            Pattern::Pulse => write!(f, "pulse"),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            ble: BleConfig {
                device_name: "Fancypants".to_string(),
                scan_timeout_secs: 30,
                reconnect_delay_secs: 5,
                data_timeout_secs: default_data_timeout_secs(),
                sample_interval_ms: None,
                notify_interval_ms: None,
            },
            mapping: MappingConfig {
                mode: MappingMode::Distance,
                invert: true, // closer = more intense
                max_speed_mm_s: default_max_speed_mm_s(),
                min_range_mm: 30,
                max_range_mm: 300,
                min_intensity: 0.0,
                max_intensity: 1.0,
                deadzone_mm: 500,
                smoothing: 0.3,
            },
            buttplug: ButtplugConfig {
                mode: EngineMode::Intiface,
                server_address: default_server_address(),
                device_index: None,
                actuator_types: default_actuator_types(),
                devices: Vec::new(),
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

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn save_default(path: &Path) -> anyhow::Result<()> {
        std::fs::write(path, DEFAULT_CONFIG_TOML)?;
        Ok(())
    }

    /// Return the list of resolved target device configurations.
    /// If `[[buttplug.devices]]` is empty, returns a single target built from
    /// top-level `buttplug.device_index`, `buttplug.actuator_types`, and `[mapping]`.
    pub fn resolved_device_targets(&self) -> Vec<ResolvedDeviceTarget> {
        if self.buttplug.devices.is_empty() {
            vec![ResolvedDeviceTarget {
                device_index: self.buttplug.device_index,
                device_name: None,
                actuator_types: self.buttplug.actuator_types.clone(),
                mapping: self.mapping.clone(),
            }]
        } else {
            self.buttplug
                .devices
                .iter()
                .map(|d| ResolvedDeviceTarget {
                    device_index: d.device_index,
                    device_name: d.device_name.clone(),
                    actuator_types: d.actuator_types.clone(),
                    mapping: d
                        .mapping
                        .as_ref()
                        .map(|m| m.resolve(&self.mapping))
                        .unwrap_or_else(|| self.mapping.clone()),
                })
                .collect()
        }
    }

    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        if let Some(ms) = self.ble.sample_interval_ms {
            if !(10..=5000).contains(&ms) {
                anyhow::bail!("ble.sample_interval_ms must be 10-5000");
            }
        }
        if let Some(ms) = self.ble.notify_interval_ms {
            if !(10..=5000).contains(&ms) {
                anyhow::bail!("ble.notify_interval_ms must be 10-5000");
            }
        }
        self.mapping.validate()?;

        if self.buttplug.mode == EngineMode::Intiface
            && self.buttplug.server_address.trim().is_empty()
        {
            anyhow::bail!("buttplug.server_address must not be empty in intiface mode");
        }
        for target in self.resolved_device_targets() {
            target.mapping.validate()?;
            if let Some(name) = &target.device_name {
                if name.trim().is_empty() {
                    anyhow::bail!("device_name must not be empty");
                }
            }
            validate_actuators(&target.actuator_types)?;
        }
        Ok(())
    }
}

fn validate_actuators(actuators: &[ActuatorConfig]) -> anyhow::Result<()> {
    if actuators.is_empty() {
        anyhow::bail!("actuator_types must list at least one actuator");
    }
    for actuator in actuators {
        let s = actuator.settings();
        s.parsed_type()?;
        if s.min_intensity < 0.0 || s.min_intensity > 1.0 {
            anyhow::bail!(
                "actuator {}: min_intensity must be 0.0-1.0",
                s.actuator_type
            );
        }
        if s.max_intensity < 0.0 || s.max_intensity > 1.0 {
            anyhow::bail!(
                "actuator {}: max_intensity must be 0.0-1.0",
                s.actuator_type
            );
        }
        if s.min_intensity > s.max_intensity {
            anyhow::bail!(
                "actuator {}: min_intensity must be <= max_intensity",
                s.actuator_type
            );
        }
        if s.pattern == Pattern::Pulse && (s.pulse_hz <= 0.0 || s.pulse_hz > 10.0) {
            anyhow::bail!(
                "actuator {}: pulse_hz must be > 0 and <= 10 (sensor updates \
                 arrive at ~20Hz, faster pulses can't be rendered)",
                s.actuator_type
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn valid_toml() -> String {
        toml::to_string_pretty(&Config::default()).unwrap()
    }

    /// A config as it looked before data_timeout_secs and actuator_types
    /// existed, without any newer fields.
    fn old_style_toml() -> String {
        r#"
[ble]
device_name = "Fancypants"
scan_timeout_secs = 30
reconnect_delay_secs = 5

[mapping]
invert = true
min_range_mm = 30
max_range_mm = 300
min_intensity = 0.0
max_intensity = 1.0
deadzone_mm = 500
smoothing = 0.3

[buttplug]
server_address = "ws://127.0.0.1:12345"
"#
        .to_string()
    }

    #[test]
    fn test_default_config_is_valid() {
        let config = Config::default();
        config.validate().unwrap();
    }

    #[test]
    fn test_load_valid_toml() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(valid_toml().as_bytes()).unwrap();
        let config = Config::load(f.path()).unwrap();
        assert_eq!(config.ble.device_name, "Fancypants");
        assert_eq!(config.mapping.min_range_mm, 30);
    }

    #[test]
    fn test_load_old_config_without_new_fields() {
        // Configs written before data_timeout_secs and actuator_types existed
        // must still load with defaults
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(old_style_toml().as_bytes()).unwrap();
        let config = Config::load(f.path()).unwrap();
        assert_eq!(config.ble.data_timeout_secs, 5);
        assert_eq!(config.buttplug.actuator_types.len(), 1);
        assert_eq!(
            config.buttplug.actuator_types[0].settings().actuator_type,
            "Vibrate"
        );
    }

    #[test]
    fn test_actuator_types_shorthand_strings() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        let toml = old_style_toml() + "actuator_types = [\"Vibrate\", \"Oscillate\"]\n";
        f.write_all(toml.as_bytes()).unwrap();
        let config = Config::load(f.path()).unwrap();
        assert_eq!(config.buttplug.actuator_types.len(), 2);
        let s = config.buttplug.actuator_types[1].settings();
        assert_eq!(s.actuator_type, "Oscillate");
        assert_eq!(s.parsed_type().unwrap(), ActuatorType::Oscillate);
        // Shorthand gets full-range constant defaults
        assert!((s.min_intensity - 0.0).abs() < f64::EPSILON);
        assert!((s.max_intensity - 1.0).abs() < f64::EPSILON);
        assert_eq!(s.pattern, Pattern::Constant);
    }

    #[test]
    fn test_actuator_types_detailed_and_mixed() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        let toml = old_style_toml()
            + "actuator_types = [\"Vibrate\", \
               { type = \"Rotate\", min_intensity = 0.2, max_intensity = 0.8, \
               pattern = \"pulse\", pulse_hz = 2.5 }]\n";
        f.write_all(toml.as_bytes()).unwrap();
        let config = Config::load(f.path()).unwrap();
        assert_eq!(config.buttplug.actuator_types.len(), 2);
        let s = config.buttplug.actuator_types[1].settings();
        assert_eq!(s.parsed_type().unwrap(), ActuatorType::Rotate);
        assert!((s.min_intensity - 0.2).abs() < f64::EPSILON);
        assert!((s.max_intensity - 0.8).abs() < f64::EPSILON);
        assert_eq!(s.pattern, Pattern::Pulse);
        assert!((s.pulse_hz - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_actuator_type_case_insensitive() {
        let s = ActuatorConfig::Type("vibrate".to_string()).settings();
        assert_eq!(s.parsed_type().unwrap(), ActuatorType::Vibrate);
    }

    #[test]
    fn test_validate_unknown_actuator_type() {
        let mut config = Config::default();
        config.buttplug.actuator_types = vec![ActuatorConfig::Type("Explode".to_string())];
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("Explode") || err.contains("explode"), "{err}");
    }

    #[test]
    fn test_validate_empty_actuator_types() {
        let mut config = Config::default();
        config.buttplug.actuator_types = vec![];
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_actuator_intensity_bounds() {
        let mut config = Config::default();
        config.buttplug.actuator_types = vec![ActuatorConfig::Settings(ActuatorSettings {
            actuator_type: "Vibrate".to_string(),
            min_intensity: 0.5,
            max_intensity: 0.4, // min > max
            pattern: Pattern::Constant,
            pulse_hz: 1.0,
        })];
        assert!(config.validate().is_err());

        config.buttplug.actuator_types = vec![ActuatorConfig::Settings(ActuatorSettings {
            actuator_type: "Vibrate".to_string(),
            min_intensity: 0.0,
            max_intensity: 1.5,
            pattern: Pattern::Constant,
            pulse_hz: 1.0,
        })];
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_pulse_hz_bounds() {
        let mut config = Config::default();
        config.buttplug.actuator_types = vec![ActuatorConfig::Settings(ActuatorSettings {
            actuator_type: "Vibrate".to_string(),
            min_intensity: 0.0,
            max_intensity: 1.0,
            pattern: Pattern::Pulse,
            pulse_hz: 0.0,
        })];
        assert!(config.validate().is_err());

        // pulse_hz is ignored for constant pattern
        config.buttplug.actuator_types = vec![ActuatorConfig::Settings(ActuatorSettings {
            actuator_type: "Vibrate".to_string(),
            min_intensity: 0.0,
            max_intensity: 1.0,
            pattern: Pattern::Constant,
            pulse_hz: 0.0,
        })];
        config.validate().unwrap();
    }

    #[test]
    fn test_old_config_defaults_to_distance_mode() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(old_style_toml().as_bytes()).unwrap();
        let config = Config::load(f.path()).unwrap();
        assert_eq!(config.mapping.mode, MappingMode::Distance);
        assert!((config.mapping.max_speed_mm_s - 500.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_load_velocity_mode() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        let toml = old_style_toml().replace(
            "[mapping]",
            "[mapping]\nmode = \"velocity\"\nmax_speed_mm_s = 800.0",
        );
        f.write_all(toml.as_bytes()).unwrap();
        let config = Config::load(f.path()).unwrap();
        assert_eq!(config.mapping.mode, MappingMode::Velocity);
        assert!((config.mapping.max_speed_mm_s - 800.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_validate_max_speed_bounds() {
        let mut config = Config::default();
        config.mapping.max_speed_mm_s = 0.0;
        assert!(config.validate().is_err());

        config.mapping.max_speed_mm_s = -100.0;
        assert!(config.validate().is_err());

        config.mapping.max_speed_mm_s = f64::INFINITY;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = Config::load(Path::new("/tmp/nonexistent_fancypants_cfg.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_invalid_toml_syntax() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"this is not [valid toml").unwrap();
        let result = Config::load(f.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_bad_values_triggers_validation() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        let toml = valid_toml().replace("min_intensity = 0.0", "min_intensity = 2.0");
        f.write_all(toml.as_bytes()).unwrap();
        let result = Config::load(f.path());
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("min_intensity"),
            "error should mention min_intensity"
        );
    }

    #[test]
    fn test_save_default_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        Config::save_default(&path).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            raw.contains("# Fancypants Configuration"),
            "saved default config should preserve comments"
        );
        let loaded = Config::load(&path).unwrap();
        let default = Config::default();
        assert_eq!(loaded.ble.device_name, default.ble.device_name);
        assert_eq!(loaded.ble.sample_interval_ms, None);
        assert_eq!(loaded.ble.notify_interval_ms, None);
        assert_eq!(loaded.mapping.min_range_mm, default.mapping.min_range_mm);
        assert_eq!(loaded.mapping.max_range_mm, default.mapping.max_range_mm);
        assert!((loaded.mapping.smoothing - default.mapping.smoothing).abs() < f64::EPSILON);
    }

    #[test]
    fn test_validate_ble_intervals() {
        let mut config = Config::default();
        config.ble.sample_interval_ms = Some(5);
        assert!(config.validate().is_err());

        config.ble.sample_interval_ms = Some(5001);
        assert!(config.validate().is_err());

        config.ble.sample_interval_ms = Some(50);
        config.ble.notify_interval_ms = Some(9);
        assert!(config.validate().is_err());

        config.ble.notify_interval_ms = Some(100);
        config.validate().unwrap();
    }

    #[test]
    fn test_validate_min_intensity_out_of_range() {
        let mut config = Config::default();
        config.mapping.min_intensity = -0.1;
        assert!(config.validate().is_err());

        config.mapping.min_intensity = 1.1;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_max_intensity_out_of_range() {
        let mut config = Config::default();
        config.mapping.max_intensity = -0.1;
        assert!(config.validate().is_err());

        config.mapping.max_intensity = 1.1;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_min_range_gte_max() {
        let mut config = Config::default();
        config.mapping.min_range_mm = 300;
        config.mapping.max_range_mm = 300;
        assert!(config.validate().is_err());

        config.mapping.min_range_mm = 400;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_smoothing_out_of_range() {
        let mut config = Config::default();
        config.mapping.smoothing = -0.1;
        assert!(config.validate().is_err());

        config.mapping.smoothing = 1.1;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_engine_mode_standalone_and_embedded_alias() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        let toml = old_style_toml().replace("[buttplug]", "[buttplug]\nmode = \"standalone\"");
        f.write_all(toml.as_bytes()).unwrap();
        let config = Config::load(f.path()).unwrap();
        assert_eq!(config.buttplug.mode, EngineMode::Standalone);
        assert_eq!(config.buttplug.mode.to_string(), "standalone");

        let mut f2 = tempfile::NamedTempFile::new().unwrap();
        let toml2 = old_style_toml().replace("[buttplug]", "[buttplug]\nmode = \"embedded\"");
        f2.write_all(toml2.as_bytes()).unwrap();
        let config2 = Config::load(f2.path()).unwrap();
        assert_eq!(config2.buttplug.mode, EngineMode::Standalone);

        // Empty server_address is rejected in intiface mode, allowed in standalone mode
        let mut bad = Config::default();
        bad.buttplug.server_address = "   ".to_string();
        assert!(bad.validate().is_err());
        bad.buttplug.mode = EngineMode::Standalone;
        bad.validate().unwrap();
    }

    #[test]
    fn test_mode_aliases_proximity_and_speed() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        let toml = old_style_toml().replace("[mapping]", "[mapping]\nmode = \"proximity\"");
        f.write_all(toml.as_bytes()).unwrap();
        let config = Config::load(f.path()).unwrap();
        assert_eq!(config.mapping.mode, MappingMode::Distance);

        let mut f2 = tempfile::NamedTempFile::new().unwrap();
        let toml2 = old_style_toml().replace("[mapping]", "[mapping]\nmode = \"speed\"");
        f2.write_all(toml2.as_bytes()).unwrap();
        let config2 = Config::load(f2.path()).unwrap();
        assert_eq!(config2.mapping.mode, MappingMode::Velocity);
    }

    #[test]
    fn test_multi_device_config_with_per_device_mapping_override() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        let toml = old_style_toml()
            + r#"
[[buttplug.devices]]
device_name = "Lush"
actuator_types = ["Vibrate"]

[[buttplug.devices]]
device_index = 1
actuator_types = [{ type = "Oscillate", max_intensity = 0.7, pattern = "pulse", pulse_hz = 2.0 }]
[buttplug.devices.mapping]
mode = "velocity"
max_speed_mm_s = 750.0
invert = false
smoothing = 0.5
"#;
        f.write_all(toml.as_bytes()).unwrap();
        let config = Config::load(f.path()).unwrap();
        let targets = config.resolved_device_targets();
        assert_eq!(targets.len(), 2);

        // First device inherits global distance mapping
        assert_eq!(targets[0].device_name.as_deref(), Some("Lush"));
        assert_eq!(targets[0].device_index, None);
        assert_eq!(targets[0].mapping.mode, MappingMode::Distance);
        assert!(targets[0].mapping.invert);
        assert!((targets[0].mapping.smoothing - 0.3).abs() < f64::EPSILON);

        // Second device overrides mode, speed, invert, smoothing, inherits range/deadzone
        assert_eq!(targets[1].device_index, Some(1));
        assert_eq!(targets[1].mapping.mode, MappingMode::Velocity);
        assert!((targets[1].mapping.max_speed_mm_s - 750.0).abs() < f64::EPSILON);
        assert!(!targets[1].mapping.invert);
        assert!((targets[1].mapping.smoothing - 0.5).abs() < f64::EPSILON);
        assert_eq!(targets[1].mapping.deadzone_mm, 500);
    }

    #[test]
    fn test_validate_per_device_invalid_mapping_or_name() {
        let mut config = Config::default();
        config.buttplug.devices = vec![DeviceTargetConfig {
            device_index: None,
            device_name: Some("   ".to_string()),
            actuator_types: default_actuator_types(),
            mapping: None,
        }];
        assert!(config.validate().is_err());

        config.buttplug.devices = vec![DeviceTargetConfig {
            device_index: None,
            device_name: Some("Toy".to_string()),
            actuator_types: default_actuator_types(),
            mapping: Some(MappingOverride {
                smoothing: Some(2.0), // invalid
                ..Default::default()
            }),
        }];
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_boundary_values_pass() {
        let mut config = Config::default();
        config.mapping.min_intensity = 0.0;
        config.mapping.max_intensity = 1.0;
        config.mapping.smoothing = 0.0;
        config.validate().unwrap();

        config.mapping.min_intensity = 1.0;
        config.mapping.max_intensity = 0.0; // swapped but both in range
        config.mapping.smoothing = 1.0;
        config.validate().unwrap();
    }
}
