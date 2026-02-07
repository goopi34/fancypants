use buttplug::client::{device::ScalarValueCommand, ButtplugClient, ButtplugClientDevice};
use buttplug::core::connector::new_json_ws_client_connector;
use buttplug::core::message::ActuatorType;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

/// Wrapper around Buttplug client for device control
pub struct ToyController {
    client: ButtplugClient,
    target_device: Option<Arc<ButtplugClientDevice>>,
    last_intensity: f64,
}

impl ToyController {
    /// Connect to Intiface Engine via websocket
    pub async fn connect(server_address: &str) -> anyhow::Result<Self> {
        let client = ButtplugClient::new("Fancypants");

        let connector = new_json_ws_client_connector(server_address);

        client.connect(connector).await?;
        info!("Connected to Intiface Engine at {}", server_address);

        Ok(ToyController {
            client,
            target_device: None,
            last_intensity: 0.0,
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
            anyhow::bail!("No Buttplug devices found. Make sure your toy is on and paired in Intiface.");
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
                            attrs.iter().any(|a| *a.actuator_type() == ActuatorType::Vibrate)
                        })
                        .unwrap_or(false)
                })
                .or_else(|| devices.first())
                .ok_or_else(|| anyhow::anyhow!("No suitable device found"))?
                .clone()
        };

        info!(
            "Using device: {} (index {})",
            device.name(),
            device.index()
        );
        self.target_device = Some(device);
        Ok(())
    }

    /// Set vibration intensity (0.0 - 1.0) on the target device.
    /// Skips the command if intensity hasn't changed significantly.
    pub async fn set_intensity(&mut self, intensity: f64) -> anyhow::Result<()> {
        let device = self
            .target_device
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No target device"))?;

        // Skip if change is negligible (< 1% difference)
        if (intensity - self.last_intensity).abs() < 0.01 {
            return Ok(());
        }

        let clamped = intensity.clamp(0.0, 1.0);
        debug!("Setting intensity: {:.3}", clamped);

        device
            .vibrate(&ScalarValueCommand::ScalarValue(clamped))
            .await?;

        self.last_intensity = clamped;
        Ok(())
    }

    /// Stop all device output
    pub async fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(device) = &self.target_device {
            device.stop().await?;
            self.last_intensity = 0.0;
        }
        Ok(())
    }

    /// Disconnect from Intiface
    pub async fn disconnect(&self) -> anyhow::Result<()> {
        self.client.disconnect().await?;
        info!("Disconnected from Intiface Engine");
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.client.connected()
    }
}
