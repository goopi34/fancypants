use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Manager, Peripheral};
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;

// Must match firmware UUIDs
const _RANGE_SERVICE_UUID: Uuid = Uuid::from_u128(0x00000001_7272_6e67_6669_6e6465720000);
pub(crate) const RANGE_CHAR_UUID: Uuid = Uuid::from_u128(0x00000002_7272_6e67_6669_6e6465720000);
const _RANGE_CONFIG_CHAR_UUID: Uuid = Uuid::from_u128(0x00000003_7272_6e67_6669_6e6465720000);

/// Events emitted by the BLE client
#[derive(Debug, PartialEq)]
pub enum BleEvent {
    /// New range reading in mm
    RangeUpdate(u16),
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

/// Find the range characteristic in a set of discovered characteristics.
pub(crate) fn find_range_characteristic(
    chars: &std::collections::BTreeSet<btleplug::api::Characteristic>,
) -> anyhow::Result<btleplug::api::Characteristic> {
    chars
        .iter()
        .find(|c| c.uuid == RANGE_CHAR_UUID)
        .cloned()
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
    fn test_find_range_characteristic_empty() {
        let chars = std::collections::BTreeSet::new();
        assert!(find_range_characteristic(&chars).is_err());
    }
}
