use crate::config::MappingConfig;

/// Maps raw distance readings to intensity values for Buttplug devices.
pub struct RangeMapper {
    config: MappingConfig,
    smoothed_intensity: f64,
    initialized: bool,
}

impl RangeMapper {
    pub fn new(config: MappingConfig) -> Self {
        RangeMapper {
            config,
            smoothed_intensity: 0.0,
            initialized: false,
        }
    }

    /// Map a distance_mm reading to an intensity 0.0-1.0.
    ///
    /// With invert=true (default): closer = higher intensity
    /// With invert=false: further = higher intensity
    ///
    /// Applies exponential moving average smoothing.
    pub fn map(&mut self, distance_mm: u16) -> f64 {
        // Dead zone check
        if self.config.deadzone_mm > 0 && distance_mm > self.config.deadzone_mm {
            return self.apply_smoothing(0.0);
        }

        // Clamp to configured range
        let clamped = distance_mm
            .max(self.config.min_range_mm)
            .min(self.config.max_range_mm);

        // Normalize to 0.0 - 1.0
        let range_span = (self.config.max_range_mm - self.config.min_range_mm) as f64;
        let normalized = if range_span > 0.0 {
            (clamped - self.config.min_range_mm) as f64 / range_span
        } else {
            0.0
        };

        // Invert if needed (closer = higher)
        let directed = if self.config.invert {
            1.0 - normalized
        } else {
            normalized
        };

        // Scale to intensity range
        let intensity_span = self.config.max_intensity - self.config.min_intensity;
        let raw_intensity = self.config.min_intensity + (directed * intensity_span);

        self.apply_smoothing(raw_intensity.clamp(0.0, 1.0))
    }

    fn apply_smoothing(&mut self, raw: f64) -> f64 {
        if !self.initialized {
            self.smoothed_intensity = raw;
            self.initialized = true;
            return raw;
        }

        let alpha = self.config.smoothing;
        self.smoothed_intensity = alpha * self.smoothed_intensity + (1.0 - alpha) * raw;
        self.smoothed_intensity
    }

    pub fn update_config(&mut self, config: MappingConfig) {
        self.config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> MappingConfig {
        MappingConfig {
            invert: true,
            min_range_mm: 30,
            max_range_mm: 300,
            min_intensity: 0.0,
            max_intensity: 1.0,
            deadzone_mm: 500,
            smoothing: 0.0, // disable for unit tests
        }
    }

    #[test]
    fn test_closest_is_max_intensity() {
        let mut mapper = RangeMapper::new(default_config());
        let intensity = mapper.map(30);
        assert!((intensity - 1.0).abs() < 0.01, "closest should be ~1.0, got {intensity}");
    }

    #[test]
    fn test_farthest_is_min_intensity() {
        let mut mapper = RangeMapper::new(default_config());
        let intensity = mapper.map(300);
        assert!((intensity - 0.0).abs() < 0.01, "farthest should be ~0.0, got {intensity}");
    }

    #[test]
    fn test_midpoint() {
        let mut mapper = RangeMapper::new(default_config());
        let intensity = mapper.map(165); // midpoint of 30-300
        assert!(intensity > 0.4 && intensity < 0.6, "midpoint should be ~0.5, got {intensity}");
    }

    #[test]
    fn test_deadzone_returns_zero() {
        let mut mapper = RangeMapper::new(default_config());
        let intensity = mapper.map(600);
        assert!((intensity - 0.0).abs() < 0.01, "deadzone should be 0.0, got {intensity}");
    }

    #[test]
    fn test_non_inverted() {
        let mut cfg = default_config();
        cfg.invert = false;
        let mut mapper = RangeMapper::new(cfg);
        let intensity = mapper.map(30);
        assert!((intensity - 0.0).abs() < 0.01, "non-inverted closest should be ~0.0, got {intensity}");
    }
}
