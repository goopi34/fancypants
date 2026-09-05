use buttplug::core::message::ActuatorType;
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
    /// Treat the sensor as disconnected if no range data arrives for this long
    #[serde(default = "default_data_timeout_secs")]
    pub data_timeout_secs: u64,
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
    /// Intensity follows the current distance reading
    #[default]
    Distance,
    /// Intensity follows the speed at which the distance is changing
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingConfig {
    /// What drives the intensity: "distance" (current reading) or "velocity"
    /// (rate of change of the reading)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtplugConfig {
    /// Intiface Engine websocket address
    pub server_address: String,
    /// Device index to control (None = first available)
    pub device_index: Option<u32>,
    /// Actuator types to target, with optional per-actuator settings
    #[serde(default = "default_actuator_types")]
    pub actuator_types: Vec<ActuatorConfig>,
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
                server_address: "ws://127.0.0.1:12345".to_string(),
                device_index: None,
                actuator_types: default_actuator_types(),
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

    pub(crate) fn validate(&self) -> anyhow::Result<()> {
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
        if !self.mapping.max_speed_mm_s.is_finite() || self.mapping.max_speed_mm_s <= 0.0 {
            anyhow::bail!("max_speed_mm_s must be > 0");
        }
        if self.buttplug.actuator_types.is_empty() {
            anyhow::bail!("actuator_types must list at least one actuator");
        }
        for actuator in &self.buttplug.actuator_types {
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
        let loaded = Config::load(&path).unwrap();
        let default = Config::default();
        assert_eq!(loaded.ble.device_name, default.ble.device_name);
        assert_eq!(loaded.mapping.min_range_mm, default.mapping.min_range_mm);
        assert_eq!(loaded.mapping.max_range_mm, default.mapping.max_range_mm);
        assert!((loaded.mapping.smoothing - default.mapping.smoothing).abs() < f64::EPSILON);
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
