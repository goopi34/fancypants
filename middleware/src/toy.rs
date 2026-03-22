use buttplug::client::{device::ScalarValueCommand, ButtplugClient, ButtplugClientDevice};
use buttplug::core::connector::new_json_ws_client_connector;
use buttplug::core::message::ActuatorType;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

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
    async fn vibrate(&self, intensity: f64) -> anyhow::Result<()>;
    async fn stop(&self) -> anyhow::Result<()>;
}

/// Real Buttplug device handle.
struct ButtplugDeviceHandle(Arc<ButtplugClientDevice>);

#[async_trait::async_trait]
impl DeviceHandle for ButtplugDeviceHandle {
    async fn vibrate(&self, intensity: f64) -> anyhow::Result<()> {
        self.0
            .vibrate(&ScalarValueCommand::ScalarValue(intensity))
            .await?;
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        self.0.stop().await?;
        Ok(())
    }
}

/// Generic toy state with pluggable device handle, containing all testable logic.
pub(crate) struct ToyState<D: DeviceHandle> {
    device: Option<D>,
    last_intensity: f64,
    connected: bool,
}

impl<D: DeviceHandle> ToyState<D> {
    fn new(connected: bool) -> Self {
        ToyState {
            device: None,
            last_intensity: 0.0,
            connected,
        }
    }

    fn set_device(&mut self, device: D) {
        self.device = Some(device);
    }
}

#[async_trait::async_trait]
impl<D: DeviceHandle + Sync> ToyBackend for ToyState<D> {
    async fn set_intensity(&mut self, intensity: f64) -> anyhow::Result<()> {
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No target device"))?;

        if !intensity_changed(intensity, self.last_intensity) {
            return Ok(());
        }

        let clamped = intensity.clamp(0.0, 1.0);
        debug!("Setting intensity: {:.3}", clamped);

        device.vibrate(clamped).await?;
        self.last_intensity = clamped;
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(device) = &self.device {
            device.stop().await?;
            self.last_intensity = 0.0;
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

    /// Scan for and select a target device
    pub async fn find_device(&mut self, device_index: Option<u32>) -> anyhow::Result<()> {
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
            // Use first device with vibrate capability
            devices
                .iter()
                .find(|d| {
                    d.message_attributes()
                        .scalar_cmd()
                        .as_ref()
                        .map(|attrs| {
                            attrs
                                .iter()
                                .any(|a| *a.actuator_type() == ActuatorType::Vibrate)
                        })
                        .unwrap_or(false)
                })
                .or_else(|| devices.first())
                .ok_or_else(|| anyhow::anyhow!("No suitable device found"))?
                .clone()
        };

        info!("Using device: {} (index {})", device.name(), device.index());
        self.state.set_device(ButtplugDeviceHandle(device));
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct MockDevice {
        vibrations: Arc<Mutex<Vec<f64>>>,
        stopped: Arc<Mutex<bool>>,
    }

    impl MockDevice {
        fn new() -> Self {
            MockDevice {
                vibrations: Arc::new(Mutex::new(Vec::new())),
                stopped: Arc::new(Mutex::new(false)),
            }
        }
    }

    #[async_trait::async_trait]
    impl DeviceHandle for MockDevice {
        async fn vibrate(&self, intensity: f64) -> anyhow::Result<()> {
            self.vibrations.lock().unwrap().push(intensity);
            Ok(())
        }

        async fn stop(&self) -> anyhow::Result<()> {
            *self.stopped.lock().unwrap() = true;
            Ok(())
        }
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
        let device = MockDevice::new();
        let vibrations = device.vibrations.clone();
        let mut state: ToyState<MockDevice> = ToyState::new(true);
        state.set_device(device);

        state.set_intensity(0.75).await.unwrap();

        let vibs = vibrations.lock().unwrap();
        assert_eq!(vibs.len(), 1);
        assert!((vibs[0] - 0.75).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_set_intensity_dedup_skips_small_change() {
        let device = MockDevice::new();
        let vibrations = device.vibrations.clone();
        let mut state: ToyState<MockDevice> = ToyState::new(true);
        state.set_device(device);

        state.set_intensity(0.5).await.unwrap();
        state.set_intensity(0.505).await.unwrap(); // < 1% change, should skip

        let vibs = vibrations.lock().unwrap();
        assert_eq!(vibs.len(), 1);
    }

    #[tokio::test]
    async fn test_set_intensity_dedup_allows_significant_change() {
        let device = MockDevice::new();
        let vibrations = device.vibrations.clone();
        let mut state: ToyState<MockDevice> = ToyState::new(true);
        state.set_device(device);

        state.set_intensity(0.5).await.unwrap();
        state.set_intensity(0.7).await.unwrap(); // > 1% change

        let vibs = vibrations.lock().unwrap();
        assert_eq!(vibs.len(), 2);
    }

    #[tokio::test]
    async fn test_set_intensity_clamps_above_one() {
        let device = MockDevice::new();
        let vibrations = device.vibrations.clone();
        let mut state: ToyState<MockDevice> = ToyState::new(true);
        state.set_device(device);

        state.set_intensity(1.5).await.unwrap();

        let vibs = vibrations.lock().unwrap();
        assert_eq!(vibs.len(), 1);
        assert!((vibs[0] - 1.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_set_intensity_clamps_below_zero() {
        let device = MockDevice::new();
        let vibrations = device.vibrations.clone();
        let mut state: ToyState<MockDevice> = ToyState::new(true);
        state.set_device(device);

        state.set_intensity(-0.5).await.unwrap();

        let vibs = vibrations.lock().unwrap();
        assert_eq!(vibs.len(), 1);
        assert!((vibs[0] - 0.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_set_intensity_no_device_errors() {
        let mut state: ToyState<MockDevice> = ToyState::new(true);

        let result = state.set_intensity(0.5).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No target device"));
    }

    #[tokio::test]
    async fn test_stop_calls_device_stop() {
        let device = MockDevice::new();
        let stopped = device.stopped.clone();
        let mut state: ToyState<MockDevice> = ToyState::new(true);
        state.set_device(device);

        state.set_intensity(0.5).await.unwrap();
        state.stop().await.unwrap();

        assert!(*stopped.lock().unwrap());
        // last_intensity should be reset
        // Verify by sending same intensity again - should send because it changed from 0.0
        // (stop resets last_intensity to 0.0)
    }

    #[tokio::test]
    async fn test_stop_without_device_is_ok() {
        let mut state: ToyState<MockDevice> = ToyState::new(true);
        state.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_stop_resets_last_intensity() {
        let device = MockDevice::new();
        let vibrations = device.vibrations.clone();
        let mut state: ToyState<MockDevice> = ToyState::new(true);
        state.set_device(device);

        state.set_intensity(0.5).await.unwrap();
        state.stop().await.unwrap();
        state.set_intensity(0.5).await.unwrap(); // should send because last was reset to 0

        let vibs = vibrations.lock().unwrap();
        assert_eq!(vibs.len(), 2); // both 0.5 sends should go through
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
