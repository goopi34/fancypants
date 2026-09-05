use crate::config::{ActuatorConfig, Pattern};
use buttplug::client::{device::ScalarCommand, ButtplugClient, ButtplugClientDevice};
use buttplug::core::connector::new_json_ws_client_connector;
use buttplug::core::message::ActuatorType;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Trait abstracting toy control for testability
#[async_trait::async_trait]
pub trait ToyBackend: Send {
    async fn set_intensity(&mut self, intensity: f64) -> anyhow::Result<()>;
    async fn stop(&mut self) -> anyhow::Result<()>;
    async fn disconnect(&self) -> anyhow::Result<()>;
    fn is_connected(&self) -> bool;
}

/// Trait wrapping the raw device commands, for testability.
#[async_trait::async_trait]
pub(crate) trait DeviceHandle: Send {
    /// Set scalar actuators: (ScalarCmd index, value, actuator type) per entry.
    async fn set_scalars(&self, values: &[(u32, f64, ActuatorType)]) -> anyhow::Result<()>;
    async fn stop(&self) -> anyhow::Result<()>;
}

/// Real Buttplug device handle.
struct ButtplugDeviceHandle(Arc<ButtplugClientDevice>);

#[async_trait::async_trait]
impl DeviceHandle for ButtplugDeviceHandle {
    async fn set_scalars(&self, values: &[(u32, f64, ActuatorType)]) -> anyhow::Result<()> {
        let map: HashMap<u32, (f64, ActuatorType)> = values
            .iter()
            .map(|(idx, value, actuator)| (*idx, (*value, *actuator)))
            .collect();
        self.0.scalar(&ScalarCommand::ScalarMap(map)).await?;
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        self.0.stop().await?;
        Ok(())
    }
}

/// An actuator config entry resolved to a concrete actuator type.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedActuator {
    pub actuator_type: ActuatorType,
    pub min_intensity: f64,
    pub max_intensity: f64,
    pub pattern: Pattern,
    pub pulse_hz: f64,
}

/// A device actuator we will drive: its ScalarCmd index plus the settings
/// from the config entry matched to it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlannedActuator {
    /// Index into the device's ScalarCmd attribute array
    pub index: u32,
    pub config: ResolvedActuator,
}

impl PlannedActuator {
    /// Compute this actuator's output for the mapped intensity (0.0-1.0) at
    /// `elapsed_secs` since the session started (used for time-based patterns).
    fn value_for(&self, intensity: f64, elapsed_secs: f64) -> f64 {
        let gated = match self.config.pattern {
            Pattern::Constant => intensity,
            Pattern::Pulse => {
                if (elapsed_secs * self.config.pulse_hz).fract() < 0.5 {
                    intensity
                } else {
                    0.0
                }
            }
        };
        if gated <= 0.0 {
            0.0
        } else {
            let span = self.config.max_intensity - self.config.min_intensity;
            (self.config.min_intensity + gated * span).clamp(0.0, 1.0)
        }
    }
}

/// Parse and normalize the configured actuator entries.
pub(crate) fn resolve_actuators(
    configs: &[ActuatorConfig],
) -> anyhow::Result<Vec<ResolvedActuator>> {
    configs
        .iter()
        .map(|c| {
            let s = c.settings();
            Ok(ResolvedActuator {
                actuator_type: s.parsed_type()?,
                min_intensity: s.min_intensity,
                max_intensity: s.max_intensity,
                pattern: s.pattern,
                pulse_hz: s.pulse_hz,
            })
        })
        .collect()
}

/// Match a device's scalar actuators (in ScalarCmd index order) against the
/// configured entries. The nth device actuator of a given type gets the nth
/// config entry of that type; extra device actuators of a type reuse the last
/// entry for that type. Device actuators of unconfigured types are left off.
pub(crate) fn plan_actuators(
    device_types: &[ActuatorType],
    configured: &[ResolvedActuator],
) -> Vec<PlannedActuator> {
    let mut plan = Vec::new();
    for (index, device_type) in device_types.iter().enumerate() {
        let matching: Vec<&ResolvedActuator> = configured
            .iter()
            .filter(|c| c.actuator_type == *device_type)
            .collect();
        if matching.is_empty() {
            continue;
        }
        let nth = device_types[..index]
            .iter()
            .filter(|t| *t == device_type)
            .count();
        let config = matching[nth.min(matching.len() - 1)].clone();
        plan.push(PlannedActuator {
            index: index as u32,
            config,
        });
    }
    plan
}

/// Generic toy state with pluggable device handle, containing all testable logic.
pub(crate) struct ToyState<D: DeviceHandle> {
    device: Option<D>,
    actuators: Vec<PlannedActuator>,
    last_values: Vec<f64>,
    connected: bool,
    started_at: Instant,
}

impl<D: DeviceHandle> ToyState<D> {
    fn new(connected: bool) -> Self {
        ToyState {
            device: None,
            actuators: Vec::new(),
            last_values: Vec::new(),
            connected,
            started_at: Instant::now(),
        }
    }

    fn set_device(&mut self, device: D) {
        self.device = Some(device);
    }

    fn set_actuators(&mut self, actuators: Vec<PlannedActuator>) {
        self.last_values = vec![0.0; actuators.len()];
        self.actuators = actuators;
    }
}

#[async_trait::async_trait]
impl<D: DeviceHandle + Sync> ToyBackend for ToyState<D> {
    async fn set_intensity(&mut self, intensity: f64) -> anyhow::Result<()> {
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No target device"))?;
        if self.actuators.is_empty() {
            anyhow::bail!("No actuators planned for device");
        }

        let clamped = intensity.clamp(0.0, 1.0);
        let elapsed = self.started_at.elapsed().as_secs_f64();
        let values: Vec<f64> = self
            .actuators
            .iter()
            .map(|a| a.value_for(clamped, elapsed))
            .collect();

        if !values_changed(&values, &self.last_values) {
            return Ok(());
        }

        debug!("Setting intensity {:.3} -> actuators {:?}", clamped, values);
        let command: Vec<(u32, f64, ActuatorType)> = self
            .actuators
            .iter()
            .zip(&values)
            .map(|(a, v)| (a.index, *v, a.config.actuator_type))
            .collect();
        device.set_scalars(&command).await?;
        self.last_values = values;
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(device) = &self.device {
            device.stop().await?;
            self.last_values.fill(0.0);
        }
        Ok(())
    }

    async fn disconnect(&self) -> anyhow::Result<()> {
        // Disconnection is handled by the controller, not the state
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

/// Wrapper around Buttplug client for device control
pub struct ToyController {
    client: ButtplugClient,
    state: ToyState<ButtplugDeviceHandle>,
}

impl ToyController {
    /// Connect to Intiface Engine via websocket
    pub async fn connect(server_address: &str) -> anyhow::Result<Self> {
        let client = ButtplugClient::new("Fancypants");
        let connector = new_json_ws_client_connector(server_address);
        client.connect(connector).await?;
        info!("Connected to Intiface Engine at {}", server_address);

        Ok(ToyController {
            state: ToyState::new(true),
            client,
        })
    }

    /// Scan for and select a target device, planning which of its actuators
    /// to drive based on the configured actuator types.
    pub async fn find_device(
        &mut self,
        device_index: Option<u32>,
        actuator_types: &[ActuatorConfig],
    ) -> anyhow::Result<()> {
        let configured = resolve_actuators(actuator_types)?;

        info!("Scanning for Buttplug devices...");
        self.client.start_scanning().await?;

        // Wait for devices to be found
        tokio::time::sleep(Duration::from_secs(5)).await;
        self.client.stop_scanning().await?;

        let devices = self.client.devices();
        if devices.is_empty() {
            anyhow::bail!(
                "No Buttplug devices found. Make sure your toy is on and paired in Intiface."
            );
        }

        let device = if let Some(idx) = device_index {
            devices
                .iter()
                .find(|d| d.index() == idx)
                .ok_or_else(|| anyhow::anyhow!("Device index {} not found", idx))?
                .clone()
        } else {
            // Use first device with at least one configured actuator type
            devices
                .iter()
                .find(|d| {
                    scalar_types(d)
                        .iter()
                        .any(|t| configured.iter().any(|c| c.actuator_type == *t))
                })
                .or_else(|| {
                    warn!("No device matches the configured actuator types, using first device");
                    devices.first()
                })
                .ok_or_else(|| anyhow::anyhow!("No suitable device found"))?
                .clone()
        };

        let device_types = scalar_types(&device);
        let plan = plan_actuators(&device_types, &configured);
        if plan.is_empty() {
            anyhow::bail!(
                "Device {} has no configured actuator types (configured: {:?}, device has: {:?})",
                device.name(),
                configured
                    .iter()
                    .map(|c| c.actuator_type)
                    .collect::<Vec<_>>(),
                device_types,
            );
        }

        info!("Using device: {} (index {})", device.name(), device.index());
        for actuator in &plan {
            info!(
                "  actuator {} ({:?}): intensity [{}-{}], pattern={}",
                actuator.index,
                actuator.config.actuator_type,
                actuator.config.min_intensity,
                actuator.config.max_intensity,
                actuator.config.pattern,
            );
        }
        self.state.set_device(ButtplugDeviceHandle(device));
        self.state.set_actuators(plan);
        Ok(())
    }
}

/// A device's scalar actuator types, in ScalarCmd index order.
fn scalar_types(device: &ButtplugClientDevice) -> Vec<ActuatorType> {
    device
        .message_attributes()
        .scalar_cmd()
        .as_ref()
        .map(|attrs| attrs.iter().map(|a| *a.actuator_type()).collect())
        .unwrap_or_default()
}

#[async_trait::async_trait]
impl ToyBackend for ToyController {
    async fn set_intensity(&mut self, intensity: f64) -> anyhow::Result<()> {
        self.state.set_intensity(intensity).await
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        self.state.stop().await
    }

    async fn disconnect(&self) -> anyhow::Result<()> {
        self.client.disconnect().await?;
        info!("Disconnected from Intiface Engine");
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.client.connected()
    }
}

/// Returns true if the intensity change is significant enough to send (>= 1%).
pub fn intensity_changed(new: f64, last: f64) -> bool {
    (new - last).abs() >= 0.01
}

/// Returns true if any per-actuator value changed enough to send.
pub(crate) fn values_changed(new: &[f64], last: &[f64]) -> bool {
    new.len() != last.len() || new.iter().zip(last).any(|(n, l)| intensity_changed(*n, *l))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct MockDevice {
        commands: Arc<Mutex<Vec<Vec<(u32, f64, ActuatorType)>>>>,
        stopped: Arc<Mutex<bool>>,
    }

    impl MockDevice {
        fn new() -> Self {
            MockDevice {
                commands: Arc::new(Mutex::new(Vec::new())),
                stopped: Arc::new(Mutex::new(false)),
            }
        }
    }

    #[async_trait::async_trait]
    impl DeviceHandle for MockDevice {
        async fn set_scalars(&self, values: &[(u32, f64, ActuatorType)]) -> anyhow::Result<()> {
            self.commands.lock().unwrap().push(values.to_vec());
            Ok(())
        }

        async fn stop(&self) -> anyhow::Result<()> {
            *self.stopped.lock().unwrap() = true;
            Ok(())
        }
    }

    fn resolved(actuator_type: ActuatorType) -> ResolvedActuator {
        ResolvedActuator {
            actuator_type,
            min_intensity: 0.0,
            max_intensity: 1.0,
            pattern: Pattern::Constant,
            pulse_hz: 1.0,
        }
    }

    fn planned(index: u32, config: ResolvedActuator) -> PlannedActuator {
        PlannedActuator { index, config }
    }

    /// A ToyState wired to a MockDevice with a single full-range Vibrate actuator.
    fn vibrate_state() -> (
        ToyState<MockDevice>,
        Arc<Mutex<Vec<Vec<(u32, f64, ActuatorType)>>>>,
        Arc<Mutex<bool>>,
    ) {
        let device = MockDevice::new();
        let commands = device.commands.clone();
        let stopped = device.stopped.clone();
        let mut state: ToyState<MockDevice> = ToyState::new(true);
        state.set_device(device);
        state.set_actuators(vec![planned(0, resolved(ActuatorType::Vibrate))]);
        (state, commands, stopped)
    }

    /// The single-actuator value from the nth recorded command.
    fn nth_value(commands: &Mutex<Vec<Vec<(u32, f64, ActuatorType)>>>, n: usize) -> f64 {
        commands.lock().unwrap()[n][0].1
    }

    #[test]
    fn test_intensity_changed_significant() {
        assert!(intensity_changed(0.5, 0.0));
    }

    #[test]
    fn test_intensity_changed_negligible() {
        assert!(!intensity_changed(0.5, 0.505));
    }

    #[test]
    fn test_intensity_changed_boundary() {
        assert!(intensity_changed(0.5, 0.49));
    }

    #[test]
    fn test_intensity_changed_negative_direction() {
        assert!(intensity_changed(0.0, 0.5));
    }

    #[test]
    fn test_intensity_changed_zero_diff() {
        assert!(!intensity_changed(0.5, 0.5));
    }

    // --- ToyState tests via ToyBackend trait ---

    #[tokio::test]
    async fn test_set_intensity_sends_to_device() {
        let (mut state, commands, _) = vibrate_state();

        state.set_intensity(0.75).await.unwrap();

        assert_eq!(commands.lock().unwrap().len(), 1);
        assert!((nth_value(&commands, 0) - 0.75).abs() < f64::EPSILON);
        assert_eq!(commands.lock().unwrap()[0][0].0, 0);
        assert_eq!(commands.lock().unwrap()[0][0].2, ActuatorType::Vibrate);
    }

    #[tokio::test]
    async fn test_set_intensity_dedup_skips_small_change() {
        let (mut state, commands, _) = vibrate_state();

        state.set_intensity(0.5).await.unwrap();
        state.set_intensity(0.505).await.unwrap(); // < 1% change, should skip

        assert_eq!(commands.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_set_intensity_dedup_allows_significant_change() {
        let (mut state, commands, _) = vibrate_state();

        state.set_intensity(0.5).await.unwrap();
        state.set_intensity(0.7).await.unwrap(); // > 1% change

        assert_eq!(commands.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_set_intensity_clamps_above_one() {
        let (mut state, commands, _) = vibrate_state();

        state.set_intensity(1.5).await.unwrap();

        assert_eq!(commands.lock().unwrap().len(), 1);
        assert!((nth_value(&commands, 0) - 1.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_set_intensity_clamps_below_zero() {
        let (mut state, commands, _) = vibrate_state();

        // Clamped to 0.0, same as the initial state, so nothing is sent
        state.set_intensity(-0.5).await.unwrap();
        assert_eq!(commands.lock().unwrap().len(), 0);

        // But a real value followed by a negative one sends 0.0
        state.set_intensity(0.5).await.unwrap();
        state.set_intensity(-0.5).await.unwrap();
        assert_eq!(commands.lock().unwrap().len(), 2);
        assert!((nth_value(&commands, 1) - 0.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_set_intensity_no_device_errors() {
        let mut state: ToyState<MockDevice> = ToyState::new(true);

        let result = state.set_intensity(0.5).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No target device"));
    }

    #[tokio::test]
    async fn test_set_intensity_no_actuators_errors() {
        let mut state: ToyState<MockDevice> = ToyState::new(true);
        state.set_device(MockDevice::new());

        let result = state.set_intensity(0.5).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No actuators"));
    }

    #[tokio::test]
    async fn test_stop_calls_device_stop() {
        let (mut state, _, stopped) = vibrate_state();

        state.set_intensity(0.5).await.unwrap();
        state.stop().await.unwrap();

        assert!(*stopped.lock().unwrap());
    }

    #[tokio::test]
    async fn test_stop_without_device_is_ok() {
        let mut state: ToyState<MockDevice> = ToyState::new(true);
        state.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_stop_resets_last_intensity() {
        let (mut state, commands, _) = vibrate_state();

        state.set_intensity(0.5).await.unwrap();
        state.stop().await.unwrap();
        state.set_intensity(0.5).await.unwrap(); // should send because last was reset to 0

        assert_eq!(commands.lock().unwrap().len(), 2); // both 0.5 sends should go through
    }

    // --- Multi-actuator routing ---

    #[tokio::test]
    async fn test_multi_actuator_per_actuator_settings() {
        let device = MockDevice::new();
        let commands = device.commands.clone();
        let mut state: ToyState<MockDevice> = ToyState::new(true);
        state.set_device(device);
        state.set_actuators(vec![
            planned(0, resolved(ActuatorType::Vibrate)),
            planned(
                2,
                ResolvedActuator {
                    actuator_type: ActuatorType::Oscillate,
                    min_intensity: 0.2,
                    max_intensity: 0.6,
                    pattern: Pattern::Constant,
                    pulse_hz: 1.0,
                },
            ),
        ]);

        state.set_intensity(0.5).await.unwrap();

        let cmds = commands.lock().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].len(), 2);
        // Vibrate at index 0: full range, follows intensity directly
        assert_eq!(cmds[0][0], (0, 0.5, ActuatorType::Vibrate));
        // Oscillate at index 2: rescaled into [0.2, 0.6] -> 0.2 + 0.5*0.4 = 0.4
        assert_eq!(cmds[0][1].0, 2);
        assert!((cmds[0][1].1 - 0.4).abs() < 1e-9);
        assert_eq!(cmds[0][1].2, ActuatorType::Oscillate);
    }

    #[tokio::test]
    async fn test_multi_actuator_dedup_sends_when_any_changes() {
        let device = MockDevice::new();
        let commands = device.commands.clone();
        let mut state: ToyState<MockDevice> = ToyState::new(true);
        state.set_device(device);
        state.set_actuators(vec![
            planned(0, resolved(ActuatorType::Vibrate)),
            // Second actuator barely moves (span 0.01), first moves normally
            planned(
                1,
                ResolvedActuator {
                    actuator_type: ActuatorType::Rotate,
                    min_intensity: 0.5,
                    max_intensity: 0.51,
                    pattern: Pattern::Constant,
                    pulse_hz: 1.0,
                },
            ),
        ]);

        state.set_intensity(0.3).await.unwrap();
        state.set_intensity(0.6).await.unwrap(); // rotate change < 1%, vibrate > 1%

        assert_eq!(commands.lock().unwrap().len(), 2);
    }

    // --- value_for (per-actuator scaling and patterns) ---

    #[test]
    fn test_value_for_zero_intensity_is_off_despite_min() {
        let actuator = planned(
            0,
            ResolvedActuator {
                actuator_type: ActuatorType::Vibrate,
                min_intensity: 0.3,
                max_intensity: 1.0,
                pattern: Pattern::Constant,
                pulse_hz: 1.0,
            },
        );
        assert_eq!(actuator.value_for(0.0, 0.0), 0.0);
        // Just above zero jumps to the actuator's floor
        assert!(actuator.value_for(0.01, 0.0) >= 0.3);
    }

    #[test]
    fn test_value_for_pulse_gates_on_and_off() {
        let actuator = planned(
            0,
            ResolvedActuator {
                actuator_type: ActuatorType::Vibrate,
                min_intensity: 0.0,
                max_intensity: 1.0,
                pattern: Pattern::Pulse,
                pulse_hz: 1.0, // 1s period: on for first 0.5s, off for next 0.5s
            },
        );
        assert!((actuator.value_for(0.8, 0.1) - 0.8).abs() < f64::EPSILON);
        assert_eq!(actuator.value_for(0.8, 0.6), 0.0);
        assert!((actuator.value_for(0.8, 1.2) - 0.8).abs() < f64::EPSILON);
    }

    // --- resolve_actuators / plan_actuators ---

    #[test]
    fn test_resolve_actuators_rejects_unknown_type() {
        let result = resolve_actuators(&[ActuatorConfig::Type("Warp".to_string())]);
        assert!(result.is_err());
    }

    #[test]
    fn test_plan_matches_by_type_and_skips_unconfigured() {
        let device_types = [
            ActuatorType::Vibrate,
            ActuatorType::Oscillate,
            ActuatorType::Vibrate,
        ];
        let configured = [resolved(ActuatorType::Vibrate)];

        let plan = plan_actuators(&device_types, &configured);

        // Both Vibrate actuators get the single Vibrate entry; Oscillate is left off
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].index, 0);
        assert_eq!(plan[1].index, 2);
        assert!(plan
            .iter()
            .all(|p| p.config.actuator_type == ActuatorType::Vibrate));
    }

    #[test]
    fn test_plan_nth_entry_matches_nth_actuator_of_type() {
        let device_types = [ActuatorType::Vibrate, ActuatorType::Vibrate];
        let first = resolved(ActuatorType::Vibrate);
        let second = ResolvedActuator {
            max_intensity: 0.5,
            ..resolved(ActuatorType::Vibrate)
        };
        let configured = [first.clone(), second.clone()];

        let plan = plan_actuators(&device_types, &configured);

        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].config, first);
        assert_eq!(plan[1].config, second);
    }

    #[test]
    fn test_plan_empty_when_no_type_matches() {
        let device_types = [ActuatorType::Oscillate];
        let configured = [resolved(ActuatorType::Vibrate)];
        assert!(plan_actuators(&device_types, &configured).is_empty());
    }

    // --- values_changed ---

    #[test]
    fn test_values_changed_length_mismatch() {
        assert!(values_changed(&[0.5], &[]));
    }

    #[test]
    fn test_values_changed_any_element() {
        assert!(!values_changed(&[0.5, 0.5], &[0.5, 0.505]));
        assert!(values_changed(&[0.5, 0.6], &[0.5, 0.5]));
    }

    #[tokio::test]
    async fn test_disconnect_is_ok() {
        let state: ToyState<MockDevice> = ToyState::new(true);
        state.disconnect().await.unwrap();
    }

    #[tokio::test]
    async fn test_is_connected() {
        let state_connected: ToyState<MockDevice> = ToyState::new(true);
        assert!(state_connected.is_connected());

        let state_disconnected: ToyState<MockDevice> = ToyState::new(false);
        assert!(!state_disconnected.is_connected());
    }
}
