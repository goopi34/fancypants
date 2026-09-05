use crate::config::{MappingConfig, MappingMode};
use std::time::Instant;

/// Maps raw distance readings to intensity values for Buttplug devices.
pub struct RangeMapper {
    config: MappingConfig,
    smoothed_intensity: f64,
    initialized: bool,
    last_sample: Option<(Instant, u16)>,
}

impl RangeMapper {
    pub fn new(config: MappingConfig) -> Self {
        RangeMapper {
            config,
            smoothed_intensity: 0.0,
            initialized: false,
            last_sample: None,
        }
    }

    /// Map a distance_mm reading to an intensity 0.0-1.0.
    ///
    /// In distance mode with invert=true (default): closer = higher intensity;
    /// with invert=false: further = higher intensity.
    /// In velocity mode: faster movement (either direction) = higher intensity.
    ///
    /// Applies exponential moving average smoothing.
    pub fn map(&mut self, distance_mm: u16) -> f64 {
        self.map_at(distance_mm, Instant::now())
    }

    fn map_at(&mut self, distance_mm: u16, now: Instant) -> f64 {
        let last_sample = self.last_sample.replace((now, distance_mm));

        // Dead zone check
        if self.config.deadzone_mm > 0 && distance_mm > self.config.deadzone_mm {
            return self.apply_smoothing(0.0);
        }

        let directed = match self.config.mode {
            MappingMode::Distance => self.normalized_distance(distance_mm),
            MappingMode::Velocity => self.normalized_speed(distance_mm, now, last_sample),
        };

        // Scale to intensity range
        let intensity_span = self.config.max_intensity - self.config.min_intensity;
        let raw_intensity = self.config.min_intensity + (directed * intensity_span);

        self.apply_smoothing(raw_intensity.clamp(0.0, 1.0))
    }

    /// Normalize a reading to 0.0-1.0 within the configured range window,
    /// honoring `invert`.
    fn normalized_distance(&self, distance_mm: u16) -> f64 {
        let clamped = distance_mm
            .max(self.config.min_range_mm)
            .min(self.config.max_range_mm);

        let range_span = (self.config.max_range_mm - self.config.min_range_mm) as f64;
        let normalized = if range_span > 0.0 {
            (clamped - self.config.min_range_mm) as f64 / range_span
        } else {
            0.0
        };

        if self.config.invert {
            1.0 - normalized
        } else {
            normalized
        }
    }

    /// Normalize the speed of range change to 0.0-1.0, where
    /// `max_speed_mm_s` and above maps to 1.0. Direction is ignored.
    fn normalized_speed(
        &self,
        distance_mm: u16,
        now: Instant,
        last_sample: Option<(Instant, u16)>,
    ) -> f64 {
        let Some((last_time, last_distance)) = last_sample else {
            return 0.0;
        };
        let dt = now.duration_since(last_time).as_secs_f64();
        if dt <= 0.0 {
            return 0.0;
        }
        let speed = (distance_mm as f64 - last_distance as f64).abs() / dt;
        (speed / self.config.max_speed_mm_s).clamp(0.0, 1.0)
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
            mode: MappingMode::Distance,
            invert: true,
            max_speed_mm_s: 500.0,
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

    fn velocity_config() -> MappingConfig {
        let mut cfg = default_config();
        cfg.mode = MappingMode::Velocity;
        cfg.max_speed_mm_s = 500.0;
        cfg.deadzone_mm = 0;
        cfg
    }

    /// Feed `distances` at fixed `step` intervals and return the last intensity.
    fn feed_at_intervals(
        mapper: &mut RangeMapper,
        distances: &[u16],
        step: std::time::Duration,
    ) -> f64 {
        let start = Instant::now();
        let mut last = 0.0;
        for (i, &d) in distances.iter().enumerate() {
            last = mapper.map_at(d, start + step * i as u32);
        }
        last
    }

    #[test]
    fn test_velocity_first_sample_is_zero() {
        let mut mapper = RangeMapper::new(velocity_config());
        let intensity = mapper.map_at(100, Instant::now());
        assert!(
            (intensity - 0.0).abs() < 0.01,
            "first sample has no velocity, got {intensity}"
        );
    }

    #[test]
    fn test_velocity_still_hand_is_zero() {
        let mut mapper = RangeMapper::new(velocity_config());
        let intensity = feed_at_intervals(
            &mut mapper,
            &[150, 150, 150, 150],
            std::time::Duration::from_millis(50),
        );
        assert!(
            (intensity - 0.0).abs() < 0.01,
            "no movement should be 0.0, got {intensity}"
        );
    }

    #[test]
    fn test_velocity_max_speed_is_full_intensity() {
        let mut mapper = RangeMapper::new(velocity_config());
        // 25mm per 50ms = 500mm/s = max_speed_mm_s
        let intensity = feed_at_intervals(
            &mut mapper,
            &[100, 125],
            std::time::Duration::from_millis(50),
        );
        assert!(
            (intensity - 1.0).abs() < 0.01,
            "moving at max_speed should be ~1.0, got {intensity}"
        );
    }

    #[test]
    fn test_velocity_half_speed_is_half_intensity() {
        let mut mapper = RangeMapper::new(velocity_config());
        // 12.5mm per 50ms = 250mm/s = half of max_speed_mm_s (round to 13mm)
        let intensity = feed_at_intervals(
            &mut mapper,
            &[100, 113],
            std::time::Duration::from_millis(50),
        );
        assert!(
            intensity > 0.4 && intensity < 0.6,
            "half speed should be ~0.5, got {intensity}"
        );
    }

    #[test]
    fn test_velocity_clamps_above_max_speed() {
        let mut mapper = RangeMapper::new(velocity_config());
        // 200mm per 50ms = 4000mm/s, way above max
        let intensity = feed_at_intervals(
            &mut mapper,
            &[100, 300],
            std::time::Duration::from_millis(50),
        );
        assert!(
            (intensity - 1.0).abs() < 0.01,
            "above max_speed should clamp to 1.0, got {intensity}"
        );
    }

    #[test]
    fn test_velocity_direction_agnostic() {
        let step = std::time::Duration::from_millis(50);
        let mut toward = RangeMapper::new(velocity_config());
        let mut away = RangeMapper::new(velocity_config());
        let a = feed_at_intervals(&mut toward, &[200, 187], step);
        let b = feed_at_intervals(&mut away, &[187, 200], step);
        assert!(
            (a - b).abs() < 0.01,
            "moving closer ({a}) and away ({b}) should map the same"
        );
    }

    #[test]
    fn test_velocity_respects_deadzone() {
        let mut cfg = velocity_config();
        cfg.deadzone_mm = 500;
        let mut mapper = RangeMapper::new(cfg);
        // Fast movement, but entirely beyond the deadzone
        let intensity = feed_at_intervals(
            &mut mapper,
            &[600, 700, 800],
            std::time::Duration::from_millis(50),
        );
        assert!(
            (intensity - 0.0).abs() < 0.01,
            "movement beyond deadzone should be 0.0, got {intensity}"
        );
    }

    #[test]
    fn test_velocity_zero_dt_is_zero() {
        let mut mapper = RangeMapper::new(velocity_config());
        let t = Instant::now();
        mapper.map_at(100, t);
        let intensity = mapper.map_at(300, t);
        assert!(
            (intensity - 0.0).abs() < 0.01,
            "zero dt should not divide by zero, got {intensity}"
        );
    }

    #[test]
    fn test_velocity_scales_to_intensity_range() {
        let mut cfg = velocity_config();
        cfg.min_intensity = 0.2;
        cfg.max_intensity = 0.8;
        let mut mapper = RangeMapper::new(cfg);
        let step = std::time::Duration::from_millis(50);
        let still = feed_at_intervals(&mut mapper, &[100, 100], step);
        assert!(
            (still - 0.2).abs() < 0.01,
            "still should be min_intensity 0.2, got {still}"
        );
        let mut mapper = RangeMapper::new({
            let mut cfg = velocity_config();
            cfg.min_intensity = 0.2;
            cfg.max_intensity = 0.8;
            cfg
        });
        let fast = feed_at_intervals(&mut mapper, &[100, 300], step);
        assert!(
            (fast - 0.8).abs() < 0.01,
            "max speed should be max_intensity 0.8, got {fast}"
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
