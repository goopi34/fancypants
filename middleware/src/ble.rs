use btleplug::api::{Central, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Manager, Peripheral};
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;

// Must match firmware UUIDs
const _RANGE_SERVICE_UUID: Uuid = Uuid::from_u128(0x00000001_7272_6e67_6669_6e6465720000);
pub(crate) const RANGE_CHAR_UUID: Uuid = Uuid::from_u128(0x00000002_7272_6e67_6669_6e6465720000);
pub(crate) const RANGE_CONFIG_CHAR_UUID: Uuid =
    Uuid::from_u128(0x00000003_7272_6e67_6669_6e6465720000);
/// Standard BLE Battery Level characteristic UUID (0x2A19)
pub(crate) const BATTERY_LEVEL_CHAR_UUID: Uuid =
    Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);

/// Firmware range_config struct (8 bytes, little-endian)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareRangeConfig {
    pub sample_interval_ms: u16,
    pub notify_interval_ms: u16,
    pub max_range_mm: u16,
    pub min_range_mm: u16,
}

impl FirmwareRangeConfig {
    pub fn from_le_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 {
            return None;
        }
        Some(FirmwareRangeConfig {
            sample_interval_ms: u16::from_le_bytes([bytes[0], bytes[1]]),
            notify_interval_ms: u16::from_le_bytes([bytes[2], bytes[3]]),
            max_range_mm: u16::from_le_bytes([bytes[4], bytes[5]]),
            min_range_mm: u16::from_le_bytes([bytes[6], bytes[7]]),
        })
    }

    pub fn to_le_bytes(self) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[0..2].copy_from_slice(&self.sample_interval_ms.to_le_bytes());
        out[2..4].copy_from_slice(&self.notify_interval_ms.to_le_bytes());
        out[4..6].copy_from_slice(&self.max_range_mm.to_le_bytes());
        out[6..8].copy_from_slice(&self.min_range_mm.to_le_bytes());
        out
    }

    /// Apply optional sample/notify interval overrides, returning the updated
    /// config only if at least one field changed.
    pub fn with_overrides(
        mut self,
        sample_interval_ms: Option<u16>,
        notify_interval_ms: Option<u16>,
    ) -> Option<Self> {
        let mut changed = false;
        if let Some(s) = sample_interval_ms {
            if self.sample_interval_ms != s {
                self.sample_interval_ms = s;
                changed = true;
            }
        }
        if let Some(n) = notify_interval_ms {
            if self.notify_interval_ms != n {
                self.notify_interval_ms = n;
                changed = true;
            }
        }
        changed.then_some(self)
    }
}

/// Events emitted by the BLE client
#[derive(Debug, PartialEq)]
pub enum BleEvent {
    /// New range reading in mm
    RangeUpdate(u16),
    /// Battery level percentage (0-100)
    BatteryUpdate(u8),
    /// Connection lost
    Disconnected,
    /// Connection established
    Connected,
}

/// A raw BLE notification (uuid + payload), decoupled from btleplug types.
pub(crate) struct RawNotification {
    pub uuid: Uuid,
    pub value: Vec<u8>,
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
    info!(
        "Scanning for '{}' ({}s timeout)...",
        device_name, timeout_secs
    );

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

/// Connect to the device, discover services, optionally configure firmware intervals,
/// read/subscribe to battery level, and subscribe to range notifications.
/// Sends range and battery updates through the provided channel.
pub async fn run_ble_client(
    peripheral: &Peripheral,
    sample_interval_ms: Option<u16>,
    notify_interval_ms: Option<u16>,
    tx: mpsc::UnboundedSender<BleEvent>,
) -> anyhow::Result<()> {
    // Connect
    peripheral.connect().await?;
    info!("Connected to fancypants-nrf52");
    tx.send(BleEvent::Connected)?;

    // Discover services
    peripheral.discover_services().await?;
    let chars = peripheral.characteristics();

    // Optionally apply firmware sampling / notification interval overrides
    if sample_interval_ms.is_some() || notify_interval_ms.is_some() {
        if let Some(cfg_char) = find_characteristic(&chars, RANGE_CONFIG_CHAR_UUID) {
            match peripheral.read(&cfg_char).await {
                Ok(raw) => match FirmwareRangeConfig::from_le_bytes(&raw) {
                    Some(current) => {
                        if let Some(updated) =
                            current.with_overrides(sample_interval_ms, notify_interval_ms)
                        {
                            peripheral
                                .write(&cfg_char, &updated.to_le_bytes(), WriteType::WithResponse)
                                .await?;
                            info!(
                                "Updated firmware config: sample={}ms, notify={}ms",
                                updated.sample_interval_ms, updated.notify_interval_ms
                            );
                        }
                    }
                    None => warn!(
                        "Firmware config characteristic had unexpected length ({} bytes)",
                        raw.len()
                    ),
                },
                Err(e) => warn!("Failed to read firmware config characteristic: {:#}", e),
            }
        } else {
            warn!("Firmware config characteristic not found; interval overrides ignored");
        }
    }

    // Read initial battery level and subscribe if supported
    if let Some(bat_char) = find_characteristic(&chars, BATTERY_LEVEL_CHAR_UUID) {
        if let Ok(raw) = peripheral.read(&bat_char).await {
            if let Some(ble_event) = parse_notification(BATTERY_LEVEL_CHAR_UUID, &raw) {
                let _ = tx.send(ble_event);
            }
        }
        if bat_char.properties.contains(CharPropFlags::NOTIFY) {
            if let Err(e) = peripheral.subscribe(&bat_char).await {
                debug!("Could not subscribe to battery notifications: {:#}", e);
            }
        }
    }

    // Find range characteristic
    let range_char = find_range_characteristic(&chars)?;

    info!("Found range characteristic: {:?}", range_char.uuid);

    // Subscribe to notifications
    peripheral.subscribe(&range_char).await?;
    info!("Subscribed to range notifications");

    // Listen for notifications via the extracted processing function
    let mut events = peripheral.notifications().await?;
    let stream = futures::stream::poll_fn(move |cx| events.poll_next_unpin(cx)).map(|event| {
        RawNotification {
            uuid: event.uuid,
            value: event.value,
        }
    });

    process_notifications(stream, tx).await;
    Ok(())
}

/// Find a characteristic by UUID in a set of discovered characteristics.
pub(crate) fn find_characteristic(
    chars: &std::collections::BTreeSet<btleplug::api::Characteristic>,
    uuid: Uuid,
) -> Option<btleplug::api::Characteristic> {
    chars.iter().find(|c| c.uuid == uuid).cloned()
}

/// Find the range characteristic in a set of discovered characteristics.
pub(crate) fn find_range_characteristic(
    chars: &std::collections::BTreeSet<btleplug::api::Characteristic>,
) -> anyhow::Result<btleplug::api::Characteristic> {
    find_characteristic(chars, RANGE_CHAR_UUID)
        .ok_or_else(|| anyhow::anyhow!("Range characteristic not found"))
}

/// Process a stream of raw BLE notifications, parsing and forwarding them as BleEvents.
/// Extracted from run_ble_client for testability.
pub(crate) async fn process_notifications(
    stream: impl futures::Stream<Item = RawNotification>,
    tx: mpsc::UnboundedSender<BleEvent>,
) {
    futures::pin_mut!(stream);

    while let Some(notif) = stream.next().await {
        if let Some(ble_event) = parse_notification(notif.uuid, &notif.value) {
            if let BleEvent::RangeUpdate(mm) = &ble_event {
                debug!("Range: {}mm", mm);
            }
            if tx.send(ble_event).is_err() {
                break;
            }
        }
    }

    warn!("Notification stream ended");
    let _ = tx.send(BleEvent::Disconnected);
}

/// Parse a BLE notification into a BleEvent, if applicable.
pub fn parse_notification(uuid: Uuid, value: &[u8]) -> Option<BleEvent> {
    if uuid == RANGE_CHAR_UUID && value.len() >= 2 {
        Some(BleEvent::RangeUpdate(u16::from_le_bytes([
            value[0], value[1],
        ])))
    } else if uuid == BATTERY_LEVEL_CHAR_UUID && !value.is_empty() {
        Some(BleEvent::BatteryUpdate(value[0].min(100)))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_2_byte_notification() {
        let result = parse_notification(RANGE_CHAR_UUID, &[0x64, 0x00]);
        assert_eq!(result, Some(BleEvent::RangeUpdate(100)));
    }

    #[test]
    fn test_parse_longer_payload() {
        let result = parse_notification(RANGE_CHAR_UUID, &[0xE8, 0x03, 0xFF, 0xFF]);
        assert_eq!(result, Some(BleEvent::RangeUpdate(1000)));
    }

    #[test]
    fn test_parse_wrong_uuid() {
        let wrong_uuid = Uuid::from_u128(0xDEADBEEF);
        let result = parse_notification(wrong_uuid, &[0x64, 0x00]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_too_short() {
        let result = parse_notification(RANGE_CHAR_UUID, &[0x64]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_empty_payload() {
        let result = parse_notification(RANGE_CHAR_UUID, &[]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_endianness() {
        assert_eq!(
            parse_notification(RANGE_CHAR_UUID, &[0x01, 0x00]),
            Some(BleEvent::RangeUpdate(1))
        );
        assert_eq!(
            parse_notification(RANGE_CHAR_UUID, &[0x00, 0x01]),
            Some(BleEvent::RangeUpdate(256))
        );
    }

    // --- process_notifications tests ---

    #[tokio::test]
    async fn test_process_notifications_forwards_valid() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let notifs = vec![
            RawNotification {
                uuid: RANGE_CHAR_UUID,
                value: vec![0x64, 0x00], // 100mm
            },
            RawNotification {
                uuid: RANGE_CHAR_UUID,
                value: vec![0xC8, 0x00], // 200mm
            },
        ];
        let stream = futures::stream::iter(notifs);

        process_notifications(stream, tx).await;

        assert_eq!(rx.recv().await, Some(BleEvent::RangeUpdate(100)));
        assert_eq!(rx.recv().await, Some(BleEvent::RangeUpdate(200)));
        assert_eq!(rx.recv().await, Some(BleEvent::Disconnected));
    }

    #[tokio::test]
    async fn test_process_notifications_skips_invalid() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let wrong_uuid = Uuid::from_u128(0xDEADBEEF);
        let notifs = vec![
            RawNotification {
                uuid: wrong_uuid,
                value: vec![0x64, 0x00], // wrong UUID, should be skipped
            },
            RawNotification {
                uuid: RANGE_CHAR_UUID,
                value: vec![0x01], // too short, should be skipped
            },
            RawNotification {
                uuid: RANGE_CHAR_UUID,
                value: vec![0x0A, 0x00], // 10mm, valid
            },
        ];
        let stream = futures::stream::iter(notifs);

        process_notifications(stream, tx).await;

        assert_eq!(rx.recv().await, Some(BleEvent::RangeUpdate(10)));
        assert_eq!(rx.recv().await, Some(BleEvent::Disconnected));
    }

    #[tokio::test]
    async fn test_process_notifications_stops_on_closed_receiver() {
        let (tx, rx) = mpsc::unbounded_channel();
        drop(rx); // close receiver immediately

        let notifs = vec![RawNotification {
            uuid: RANGE_CHAR_UUID,
            value: vec![0x64, 0x00],
        }];
        let stream = futures::stream::iter(notifs);

        // Should not panic, just stop
        process_notifications(stream, tx).await;
    }

    #[tokio::test]
    async fn test_process_notifications_empty_stream() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let stream = futures::stream::empty();

        process_notifications(stream, tx).await;

        // Should still send Disconnected when stream ends
        assert_eq!(rx.recv().await, Some(BleEvent::Disconnected));
    }

    // --- find_range_characteristic tests ---

    fn make_characteristic(uuid: Uuid) -> btleplug::api::Characteristic {
        btleplug::api::Characteristic {
            uuid,
            service_uuid: Uuid::from_u128(0),
            properties: btleplug::api::CharPropFlags::NOTIFY,
            descriptors: std::collections::BTreeSet::new(),
        }
    }

    #[test]
    fn test_find_range_characteristic_found() {
        let mut chars = std::collections::BTreeSet::new();
        chars.insert(make_characteristic(Uuid::from_u128(0xAAAA)));
        chars.insert(make_characteristic(RANGE_CHAR_UUID));
        chars.insert(make_characteristic(Uuid::from_u128(0xBBBB)));

        let result = find_range_characteristic(&chars).unwrap();
        assert_eq!(result.uuid, RANGE_CHAR_UUID);
    }

    #[test]
    fn test_find_range_characteristic_not_found() {
        let mut chars = std::collections::BTreeSet::new();
        chars.insert(make_characteristic(Uuid::from_u128(0xAAAA)));

        let result = find_range_characteristic(&chars);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Range characteristic not found"));
    }

    #[test]
    fn test_parse_battery_notification() {
        assert_eq!(
            parse_notification(BATTERY_LEVEL_CHAR_UUID, &[85]),
            Some(BleEvent::BatteryUpdate(85))
        );
        // Clamps above 100
        assert_eq!(
            parse_notification(BATTERY_LEVEL_CHAR_UUID, &[250]),
            Some(BleEvent::BatteryUpdate(100))
        );
        // Empty payload is ignored
        assert_eq!(parse_notification(BATTERY_LEVEL_CHAR_UUID, &[]), None);
    }

    #[test]
    fn test_firmware_range_config_roundtrip_and_overrides() {
        let orig = FirmwareRangeConfig {
            sample_interval_ms: 50,
            notify_interval_ms: 50,
            max_range_mm: 1200,
            min_range_mm: 30,
        };
        let bytes = orig.to_le_bytes();
        assert_eq!(FirmwareRangeConfig::from_le_bytes(&bytes), Some(orig));
        assert_eq!(FirmwareRangeConfig::from_le_bytes(&bytes[..7]), None);

        // No change -> None
        assert_eq!(orig.with_overrides(None, None), None);
        assert_eq!(orig.with_overrides(Some(50), Some(50)), None);

        // Override notify only
        let updated = orig.with_overrides(None, Some(100)).unwrap();
        assert_eq!(updated.sample_interval_ms, 50);
        assert_eq!(updated.notify_interval_ms, 100);
        assert_eq!(updated.max_range_mm, 1200);
        assert_eq!(updated.min_range_mm, 30);

        // Override sample only
        let updated2 = orig.with_overrides(Some(25), None).unwrap();
        assert_eq!(updated2.sample_interval_ms, 25);
        assert_eq!(updated2.notify_interval_ms, 50);
    }

    #[test]
    fn test_find_range_characteristic_empty() {
        let chars = std::collections::BTreeSet::new();
        assert!(find_range_characteristic(&chars).is_err());
    }
}
