# Firmware (fancypants-nrf52)

## Flash

**UF2 method (no programmer needed):**

1. Connect Feather to USB
2. Double-tap the reset button — `FTHR840BOOT` drive appears
3. Copy the UF2 file:

```bash
cp build/firmware/zephyr.uf2 /run/media/$USER/FTHR840BOOT/
# (or wherever the drive mounts on your system)
```

**SWD method:**

```bash
west flash
```

## Verify

Connect to the USB serial console:

```bash
# Find the USB CDC ACM device
ls /dev/ttyACM*

# Connect (115200 baud, though CDC ACM ignores baud rate)
picocom /dev/ttyACM0
# or
screen /dev/ttyACM0 115200
```

You should see:

```
[00:00:00.000,000] <inf> main: Fancypants nRF52 0.4.2 starting...
[00:00:00.050,000] <inf> main: VL53L0X sensor ready
[00:00:00.060,000] <inf> battery: Battery ADC initialized on AIN5
[00:00:00.070,000] <inf> range_svc: Range Service initialized
[00:00:00.200,000] <inf> main: Bluetooth initialized
[00:00:00.210,000] <inf> main: Advertising as "Fancypants"
[00:00:00.210,000] <inf> main: Fancypants nRF52 running
[00:00:00.210,000] <inf> main: Sensor thread started
[00:00:00.210,000] <inf> main: Battery thread started
```

## BLE Protocol

The firmware advertises as `"Fancypants"` (`CONFIG_BT_DEVICE_NAME`) and accepts
a single BLE central connection (`CONFIG_BT_MAX_CONN=1`), automatically
restarting advertising when the central disconnects.

**Custom Range Service UUID:** `00000001-7272-6e67-6669-6e6465720000`
(ASCII `"rrngfinder"` embedded)

| Characteristic | UUID                                   | Properties   | Data                        |
|----------------|----------------------------------------|--------------|-----------------------------|
| Range          | `00000002-7272-6e67-6669-6e6465720000` | Read, Notify | `uint16_t` LE, distance in mm |
| Config         | `00000003-7272-6e67-6669-6e6465720000` | Read, Write  | 8-byte struct (see below)   |

**Config struct (8 bytes, little-endian):**

| Offset | Type       | Field                | Default | Valid Range                |
|--------|------------|----------------------|---------|----------------------------|
| 0      | `uint16_t` | `sample_interval_ms` | `50`    | `10`–`5000` ms             |
| 2      | `uint16_t` | `notify_interval_ms` | `50`    | `10`–`5000` ms             |
| 4      | `uint16_t` | `max_range_mm`       | `1200`  | `> min_range_mm`           |
| 6      | `uint16_t` | `min_range_mm`       | `30`    | `< max_range_mm`           |

Sensor readings are clamped to `[min_range_mm, max_range_mm]` before updating
the Range characteristic. BLE notifications are rate-limited to
`notify_interval_ms` independently of `sample_interval_ms`. When
`ble.sample_interval_ms` or `ble.notify_interval_ms` is set in `config.toml`
(or in the GUI app), Fancypants reads the Config characteristic on connect and
writes back the updated intervals.

### Battery Service (`0x180F`)

The firmware also exposes the standard BLE **Battery Service (`0x180F`)** with
the **Battery Level (`0x2A19`)** characteristic (`uint8_t`, 0–100%, Read +
Notify).

- Sampled every 60 seconds (`CONFIG_BATTERY_SAMPLE_INTERVAL_S`) on `AIN5`
  (`P0.29`), which is connected to the Feather nRF52840's onboard `VBAT / 2`
  resistor divider.
- Converted from millivolts to percentage via a piecewise single-cell LiPo
  discharge curve (`4200 mV` = 100%, `4100 mV` = 90%, `3800 mV` = 50%,
  `3600 mV` = 20%, `3300 mV` = 0%).

## Compile-time configuration (`firmware/prj.conf` & `firmware/Kconfig`)

| Kconfig symbol                     | Default        | Description                                  |
|------------------------------------|----------------|----------------------------------------------|
| `CONFIG_BT_DEVICE_NAME`            | `"Fancypants"` | Advertised BLE peripheral name               |
| `CONFIG_RANGE_SAMPLE_INTERVAL_MS`  | `50`           | Default VL53L0X polling interval (20Hz)      |
| `CONFIG_RANGE_NOTIFY_INTERVAL_MS`  | `50`           | Default BLE notification interval (20Hz)     |
| `CONFIG_BATTERY_SAMPLE_INTERVAL_S` | `60`           | Battery ADC sampling interval in seconds     |
| `CONFIG_APP_VERSION_STRING`        | `"dev"`        | Version string printed at boot (`VERSION`)   |
