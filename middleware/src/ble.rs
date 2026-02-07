use btleplug::api::{
    Central, Manager as _, Peripheral as _, ScanFilter,
};
use btleplug::platform::{Manager, Peripheral};
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;

// Must match firmware UUIDs
const RANGE_SERVICE_UUID: Uuid = Uuid::from_u128(0x00000001_7272_6e67_6669_6e6465720000);
const RANGE_CHAR_UUID: Uuid = Uuid::from_u128(0x00000002_7272_6e67_6669_6e6465720000);
const _RANGE_CONFIG_CHAR_UUID: Uuid = Uuid::from_u128(0x00000003_7272_6e67_6669_6e6465720000);

/// Events emitted by the BLE client
#[derive(Debug)]
pub enum BleEvent {
    /// New range reading in mm
    RangeUpdate(u16),
    /// Connection lost
    Disconnected,
    /// Connection established
    Connected,
}

/// Scan for and connect to the fancypants-nrf52 peripheral
pub async fn find_device(device_name: &str, timeout_secs: u64) -> anyhow::Result<Peripheral> {
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;

    if adapters.is_empty() {
        anyhow::bail!("No Bluetooth adapters found");
    }

    let adapter = &adapters[0];
    info!("Using adapter: {:?}", adapter.adapter_info().await?);

    adapter.start_scan(ScanFilter::default()).await?;
    info!("Scanning for '{}' ({}s timeout)...", device_name, timeout_secs);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

    loop {
        if tokio::time::Instant::now() > deadline {
            adapter.stop_scan().await?;
            anyhow::bail!("Scan timeout: '{}' not found", device_name);
        }

        let peripherals = adapter.peripherals().await?;
        for p in peripherals {
            if let Some(props) = p.properties().await? {
                if props.local_name.as_deref() == Some(device_name) {
                    adapter.stop_scan().await?;
                    info!("Found device: {} ({:?})", device_name, p.id());
                    return Ok(p);
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Connect to the device, discover services, and subscribe to range notifications.
/// Sends range updates through the provided channel.
pub async fn run_ble_client(
    peripheral: &Peripheral,
    tx: mpsc::UnboundedSender<BleEvent>,
) -> anyhow::Result<()> {
    // Connect
    peripheral.connect().await?;
    info!("Connected to fancypants-nrf52");
    tx.send(BleEvent::Connected)?;

    // Discover services
    peripheral.discover_services().await?;
    let chars = peripheral.characteristics();

    // Find range characteristic
    let range_char = chars
        .iter()
        .find(|c| c.uuid == RANGE_CHAR_UUID)
        .ok_or_else(|| anyhow::anyhow!("Range characteristic not found"))?
        .clone();

    info!("Found range characteristic: {:?}", range_char.uuid);

    // Subscribe to notifications
    peripheral.subscribe(&range_char).await?;
    info!("Subscribed to range notifications");

    // Listen for notifications
    let mut events = peripheral.notifications().await?;

    while let Some(event) = events.next().await {
        if event.uuid == RANGE_CHAR_UUID {
            if event.value.len() >= 2 {
                let distance_mm = u16::from_le_bytes([event.value[0], event.value[1]]);
                debug!("Range: {}mm", distance_mm);
                if tx.send(BleEvent::RangeUpdate(distance_mm)).is_err() {
                    break;
                }
            }
        }
    }

    warn!("Notification stream ended");
    let _ = tx.send(BleEvent::Disconnected);
    Ok(())
}
