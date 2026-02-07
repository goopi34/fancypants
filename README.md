# Fancypants

A two-part system that uses an nRF52840 + VL53L0X time-of-flight sensor to control
sex toys through Intiface Engine. "Wave your hand" closer to the sensor for more
intensity, pull away to reduce it.

```
┌─────────────────────┐     BLE      ┌──────────────────┐    WebSocket   ┌─────────────┐
│  fancypants-nrf52   │ ──────────── │  fancypants      │ ────────────── │  Intiface   │
│  Feather nRF52840   │  range_mm    │  Rust middleware │  intensity     │  Engine     │
│  + VL53L0X sensor   │  + battery   │  mapping + EMA   │  ScalarCmd     │  → toy      │
└─────────────────────┘              └──────────────────┘                └─────────────┘
```

## Hardware

- **Adafruit Feather nRF52840 Express**
- **Adafruit VL53L0X Time-of-Flight sensor breakout** (I2C, 30-1200mm range)
- **LiPo battery** (optional, JST-PH connector on Feather)
- Any Buttplug-compatible toy + Intiface Central

### Wiring

If your VL53L0X has a STEMMA QT / Qwiic connector, just use the cable.
Otherwise:

| Feather | VL53L0X |
|---------|---------|
| 3V      | VIN     |
| GND     | GND     |
| SDA     | SDA     |
| SCL     | SCL     |

The Adafruit VL53L0X breakout includes I2C pullups and a voltage regulator,
so no additional components are needed.

## Building

Everything builds in containers — no local toolchains needed. Just `docker` (or `podman`) and `make`.

```bash
# Build everything
make

# Build just firmware
make firmware

# Build just middleware
make middleware

# Clean all build artifacts
make clean

# See all options
make help
```

Build outputs land in `build/`:

```
build/
├── firmware/
│   └── zephyr.uf2          ← flash this to the Feather
├── middleware/
│   └── fancypants           ← run this on your PC
└── cargo-cache/             ← persistent Rust dependency cache
```

You can override the NCS version, board target, or container runtime:

```bash
make firmware NCS_TAG=v2.7-branch
make firmware BOARD=adafruit_feather_nrf52840/nrf52840/uf2
make CONTAINER=podman
```

For interactive debugging, drop into a build container shell:

```bash
make shell-fw    # firmware (NCS/Zephyr environment)
make shell-mw    # middleware (Rust environment)
```

### Manual builds (without containers)

If you'd rather install toolchains locally:

**Firmware** — requires nRF Connect SDK (`west`):

```bash
cd firmware
west build -b adafruit_feather_nrf52840
```

**Middleware** — requires Rust toolchain + BlueZ dev libs:

```bash
cd middleware
cargo build --release
```

## Part 1: fancypants-nrf52 Firmware

### Flash

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

### Verify

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
[00:00:00.000,000] <inf> main: Fancypants nRF52 starting...
[00:00:00.050,000] <inf> main: VL53L0X sensor ready
[00:00:00.060,000] <inf> battery: Battery ADC initialized on AIN5
[00:00:00.070,000] <inf> range_svc: Range Service initialized
[00:00:00.200,000] <inf> main: Bluetooth initialized
[00:00:00.210,000] <inf> main: Advertising as "Fancypants"
```

### BLE Protocol

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

## Part 2: fancypants Middleware

### Prerequisites (runtime only)

- Intiface Central running (https://intiface.com/central/)
- BlueZ (Linux) or equivalent BLE stack

On Arch:

```bash
sudo pacman -S bluez bluez-utils
sudo systemctl enable --now bluetooth
```

### Configure

Edit `config.toml` (or run `--generate-config` to create a fresh one):

```bash
# Generate default config
./build/middleware/fancypants --generate-config

# Edit as needed
$EDITOR config.toml
```

Key settings:

- `mapping.invert = true` — closer = more intense (default)
- `mapping.min_range_mm` / `max_range_mm` — active zone
- `mapping.deadzone_mm` — pull away past this to turn off
- `mapping.smoothing` — 0.3 is a good default, increase for smoother response

### Run

1. Start Intiface Central and ensure your toy is connected
2. Power on the Feather (USB or battery)
3. Run the middleware:

```bash
./build/middleware/fancypants -c config.toml

# With debug logging:
./build/middleware/fancypants -c config.toml -l debug
```

### What happens

1. Middleware scans BLE for "Fancypants" device
2. Connects and subscribes to range notifications
3. Connects to Intiface Engine via websocket
4. Finds your toy
5. Maps distance → intensity and sends commands at ~20Hz

```
[INFO] Found device: Fancypants
[INFO] Connected to Intiface Engine at ws://127.0.0.1:12345
[INFO] Using device: We-Vibe Melt 2 (index 0)
[INFO] Running — move your hand near the sensor!
```

## Troubleshooting

**"No Bluetooth adapters found"**
- `sudo systemctl start bluetooth`
- Check `rfkill list` for blocked adapters

**"Scan timeout: 'Fancypants' not found"**
- Is the Feather powered and running? Check USB serial console
- Is another device already connected to it? (nRF52840 supports 1 connection)
- Try `bluetoothctl` → `scan on` to verify the device is advertising

**"No Buttplug devices found"**
- Open Intiface Central, make sure it's running and your toy is visible
- Check that the websocket port matches `config.toml`

**Sensor reads 0mm or 8190mm constantly**
- Check I2C wiring (SDA/SCL not swapped?)
- VL53L0X breakout getting 3.3V power?
- 8190mm typically means "out of range" or no target

**Jerky/stuttery toy response**
- Increase `mapping.smoothing` (try 0.5-0.7)
- Increase `notify_interval_ms` in firmware config characteristic

## License

MIT
