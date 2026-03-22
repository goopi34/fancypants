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

    #[allow(dead_code)]
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
        assert!(
            (intensity - 1.0).abs() < 0.01,
            "closest should be ~1.0, got {intensity}"
        );
    }

    #[test]
    fn test_farthest_is_min_intensity() {
        let mut mapper = RangeMapper::new(default_config());
        let intensity = mapper.map(300);
        assert!(
            (intensity - 0.0).abs() < 0.01,
            "farthest should be ~0.0, got {intensity}"
        );
    }

    #[test]
    fn test_midpoint() {
        let mut mapper = RangeMapper::new(default_config());
        let intensity = mapper.map(165); // midpoint of 30-300
        assert!(
            intensity > 0.4 && intensity < 0.6,
            "midpoint should be ~0.5, got {intensity}"
        );
    }

    #[test]
    fn test_deadzone_returns_zero() {
        let mut mapper = RangeMapper::new(default_config());
        let intensity = mapper.map(600);
        assert!(
            (intensity - 0.0).abs() < 0.01,
            "deadzone should be 0.0, got {intensity}"
        );
    }

    #[test]
    fn test_non_inverted() {
        let mut cfg = default_config();
        cfg.invert = false;
        let mut mapper = RangeMapper::new(cfg);
        let intensity = mapper.map(30);
        assert!(
            (intensity - 0.0).abs() < 0.01,
            "non-inverted closest should be ~0.0, got {intensity}"
        );
    }

    #[test]
    fn test_smoothing_first_call_returns_raw() {
        let mut cfg = default_config();
        cfg.smoothing = 0.5;
        let mut mapper = RangeMapper::new(cfg);
        // First call should return raw value regardless of smoothing
        let intensity = mapper.map(30); // closest -> ~1.0 inverted
        assert!(
            (intensity - 1.0).abs() < 0.01,
            "first call should return raw, got {intensity}"
        );
    }

    #[test]
    fn test_smoothing_converges() {
        let mut cfg = default_config();
        cfg.smoothing = 0.5;
        let mut mapper = RangeMapper::new(cfg);
        // Feed the same value repeatedly; EMA should converge to it
        for _ in 0..20 {
            mapper.map(165); // midpoint -> ~0.5
        }
        let intensity = mapper.map(165);
        assert!(
            (intensity - 0.5).abs() < 0.05,
            "should converge to ~0.5, got {intensity}"
        );
    }

    #[test]
    fn test_smoothing_dampens_spike() {
        let mut cfg = default_config();
        cfg.smoothing = 0.8; // heavy smoothing
        let mut mapper = RangeMapper::new(cfg);
        // Establish baseline at midpoint
        for _ in 0..20 {
            mapper.map(165);
        }
        // Spike to max
        let spiked = mapper.map(30);
        // With 0.8 smoothing, output should be much less than 1.0
        assert!(spiked < 0.7, "spike should be dampened, got {spiked}");
    }

    #[test]
    fn test_smoothing_zero_is_passthrough() {
        let mut cfg = default_config();
        cfg.smoothing = 0.0;
        let mut mapper = RangeMapper::new(cfg);
        mapper.map(165); // initialize
        let a = mapper.map(30);
        let b = mapper.map(300);
        assert!(
            (a - 1.0).abs() < 0.01,
            "smoothing=0 should pass through, got {a}"
        );
        assert!(
            (b - 0.0).abs() < 0.01,
            "smoothing=0 should pass through, got {b}"
        );
    }

    #[test]
    fn test_smoothing_one_holds_first() {
        let mut cfg = default_config();
        cfg.smoothing = 1.0; // max smoothing: output = 1.0 * prev + 0.0 * new
        let mut mapper = RangeMapper::new(cfg);
        let first = mapper.map(30); // ~1.0
                                    // All subsequent calls should stay at first value
        let second = mapper.map(300);
        let third = mapper.map(300);
        assert!(
            (first - 1.0).abs() < 0.01,
            "first should be ~1.0, got {first}"
        );
        assert!(
            (second - 1.0).abs() < 0.01,
            "smoothing=1.0 should hold first value, got {second}"
        );
        assert!(
            (third - 1.0).abs() < 0.01,
            "smoothing=1.0 should hold first value, got {third}"
        );
    }

    #[test]
    fn test_update_config_changes_behavior() {
        let mut mapper = RangeMapper::new(default_config());
        let before = mapper.map(30); // inverted -> ~1.0
        assert!((before - 1.0).abs() < 0.01);

        let mut new_cfg = default_config();
        new_cfg.invert = false;
        mapper.update_config(new_cfg);
        let after = mapper.map(30); // non-inverted -> ~0.0
        assert!(
            (after - 0.0).abs() < 0.01,
            "after config update, should be ~0.0, got {after}"
        );
    }

    #[test]
    fn test_zero_range_span() {
        let mut cfg = default_config();
        cfg.min_range_mm = 100;
        cfg.max_range_mm = 100;
        cfg.deadzone_mm = 0;
        let mut mapper = RangeMapper::new(cfg);
        let intensity = mapper.map(100);
        // With zero span, normalized = 0.0, inverted = 1.0, scaled = 1.0
        assert!(
            (intensity - 1.0).abs() < 0.01,
            "zero span inverted should be 1.0, got {intensity}"
        );
    }

    #[test]
    fn test_below_min_clamped() {
        let mut mapper = RangeMapper::new(default_config());
        // 10mm is below min_range_mm (30), should clamp to 30 -> same as closest
        let intensity = mapper.map(10);
        assert!(
            (intensity - 1.0).abs() < 0.01,
            "below min should clamp to closest, got {intensity}"
        );
    }

    #[test]
    fn test_deadzone_disabled() {
        let mut cfg = default_config();
        cfg.deadzone_mm = 0; // disabled
        let mut mapper = RangeMapper::new(cfg);
        // Far distance should still map (clamped to max_range), not return 0
        let intensity = mapper.map(1000);
        assert!(
            (intensity - 0.0).abs() < 0.01,
            "clamped to max_range (inverted) should be ~0.0, got {intensity}"
        );
    }

    #[test]
    fn test_custom_intensity_range() {
        let mut cfg = default_config();
        cfg.min_intensity = 0.2;
        cfg.max_intensity = 0.8;
        let mut mapper = RangeMapper::new(cfg);
        let closest = mapper.map(30); // inverted -> max_intensity
        let farthest = mapper.map(300); // inverted -> min_intensity
        assert!(
            (closest - 0.8).abs() < 0.01,
            "closest should be max_intensity 0.8, got {closest}"
        );
        assert!(
            (farthest - 0.2).abs() < 0.01,
            "farthest should be min_intensity 0.2, got {farthest}"
        );
    }
}
