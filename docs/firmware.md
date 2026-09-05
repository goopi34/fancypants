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
[00:00:00.000,000] <inf> main: Fancypants nRF52 0.1.4 starting...
[00:00:00.050,000] <inf> main: VL53L0X sensor ready
[00:00:00.060,000] <inf> battery: Battery ADC initialized on AIN5
[00:00:00.070,000] <inf> range_svc: Range Service initialized
[00:00:00.200,000] <inf> main: Bluetooth initialized
[00:00:00.210,000] <inf> main: Advertising as "Fancypants"
```

## BLE Protocol

**Custom Range Service UUID:** `00000001-7272-6e67-6669-6e6465720000`

| Characteristic | UUID       | Properties    | Data                          |
|----------------|------------|---------------|-------------------------------|
| Range          | ...0002... | Read, Notify  | uint16_t LE, distance in mm   |
| Config         | ...0003... | Read, Write   | 8-byte struct (see below)     |

**Config struct (8 bytes, little-endian):**

| Offset | Type     | Field              |
|--------|----------|--------------------|
| 0      | uint16_t | sample_interval_ms |
| 2      | uint16_t | notify_interval_ms |
| 4      | uint16_t | max_range_mm       |
| 6      | uint16_t | min_range_mm       |

Also exposes the standard **Battery Service (0x180F)**.
