# Middleware & GUI Application (fancypants / fancypants-app)

Fancypants ships both a **Tauri v2 GUI application** (`fancypants-app`, for
Linux, Windows, and Android) and a **headless CLI** (`fancypants`), built on
the same Rust core library (`middleware/`).

## Getting the binaries

Download prebuilt packages from the [Releases page](../../releases):

- **Linux (x86_64):**
  - `fancypants-<version>-linux-amd64.deb` — Debian/Ubuntu GUI package
  - `fancypants-<version>-linux-x86_64.rpm` — Fedora/RHEL/openSUSE GUI package
  - `fancypants-<version>-linux-x86_64.AppImage` — portable Linux GUI AppImage
  - `fancypants-<version>-linux-x86_64.tar.gz` — both `fancypants-app` (GUI)
    and `fancypants` (CLI), plus a commented sample `config.toml` and docs
- **Windows (x86_64):**
  - `fancypants-<version>-windows-x86_64.zip` — `fancypants-app.exe` (GUI),
    `fancypants.exe` (CLI), sample `config.toml`, and docs
- **Android (ARM64):**
  - `fancypants-<version>-android-arm64.apk` — signed, sideloadable Android APK

Or build from source (see [building.md](building.md)) — binaries land in
`build/app/fancypants-app` and `build/middleware/fancypants` (or run directly
via `make run-app` / `make run-cli` / `nix run`).

## Prerequisites (runtime)

### Bluetooth Low Energy (BLE)

- **Linux:** BlueZ (`bluez` and `bluez-utils`) with the `bluetooth` service active:
  ```bash
  # Arch
  sudo pacman -S bluez bluez-utils
  sudo systemctl enable --now bluetooth
  ```
- **Windows:** Windows 10 or 11 with a Bluetooth LE adapter (uses WinRT BLE).
- **Android:** Android 8.0+ (API 26+) device with BLE support. On first launch,
  grant the requested Nearby Devices (`BLUETOOTH_SCAN` / `BLUETOOTH_CONNECT`,
  or `ACCESS_FINE_LOCATION` on Android 11 and earlier) and Notification
  permissions. The app starts a `connectedDevice` foreground service
  (`FancypantsBleService`) so BLE connections to the sensor and toys stay alive
  when your phone is locked in a pocket.

### Toy Engine Mode

Fancypants supports two ways to talk to Buttplug-compatible toys:

1. **Standalone BLE (`buttplug.mode = "standalone"`):** Runs an embedded
   Buttplug server in-process and connects directly to BLE toys using your
   device's Bluetooth adapter. **No external software is needed** — ideal for
   Android or single-app desktop use.
2. **Intiface Central / Engine (`buttplug.mode = "intiface"`, default):**
   Connects to an external [Intiface Central](https://intiface.com/#intiface-central)
   instance over WebSocket (`ws://127.0.0.1:12345` by default).

## GUI Application (`fancypants-app`)

The Tauri v2 GUI wraps the middleware session engine in an interactive
dashboard:

- **Top bar:** Shows session state (`Idle`, `Scanning BLE...`, `Connecting...`,
  `Running`, `Reconnecting...`), live sensor LiPo battery percentage (`BAT`),
  and the **Start / Stop** session button.
- **Live Sensor & Output card:** Displays real-time sensor distance in `mm`, a
  range bar with visual markers for the active window (`min_range_mm`–`max_range_mm`)
  and off deadzone (`deadzone_mm`), smoothed output intensity (`%`), and live
  per-toy / per-actuator output bars once toys are connected.
- **Connection & Engine card:** Toggle between **Standalone BLE** (direct to
  toys) and **Intiface Central** (WebSocket URL). Expand **Sensor BLE Settings**
  to customize the sensor's advertised BLE name, watchdog timeout
  (`data_timeout_secs`), and firmware sampling/notification intervals
  (`sample_interval_ms` / `notify_interval_ms`, 10–5000ms).
- **Default Sensor Mapping card:** Switch between **Proximity (Distance)** and
  **Movement Speed (Velocity)** modes, toggle `Closer = More Intense` (`invert`),
  and drag sliders for active range, max speed, deadzone, EMA smoothing, and
  min/max intensity.
- **Target Toys & Actuators card:** Add one or more toys, optionally match each
  by name substring (e.g. `Lush`, `Melt`) or leave blank to claim any available
  toy, choose per-toy mapping overrides (`Use Default Mapping`,
  `Proximity: Closer = Stronger`, `Proximity: Further = Stronger`, or
  `Movement Speed (Velocity)`), and add/configure actuators (`Vibrate`,
  `Rotate`, `Oscillate`, `Constrict`, `Inflate`, `Position`, min/max intensity,
  `Constant` vs `Pulse` pattern and Hz).
- **Live hot-reload & persistence:** Adjusting mapping or actuator controls in
  the UI immediately hot-reloads the running session without disconnecting.
  Click **Save Configuration** at the bottom to write your settings to
  `config.toml`.

## Configure (`config.toml`)

The CLI looks for its config in this order:

1. The path given with `-c PATH` (must exist)
2. `./config.toml` (current working directory)
3. `$XDG_CONFIG_HOME/fancypants/config.toml` (defaults to `~/.config/fancypants/config.toml`)

(The GUI app checks the platform app config directory first, falling back to
the same `config.toml` search order.)

If no config file is found, Fancypants writes a commented default config to
`~/.config/fancypants/config.toml`, prints where it put it, and runs with those
defaults.

`fancypants --generate-config` writes the default config explicitly (to `-c PATH`
if given) and exits; it refuses to overwrite an existing file.

```bash
# Edit as needed
$EDITOR ~/.config/fancypants/config.toml
```

Key settings:

- `ble.device_name` — advertised BLE name of the sensor (`"Fancypants"` by default)
- `ble.scan_timeout_secs` / `ble.reconnect_delay_secs` — scan timeout (default 30s)
  and delay before reconnecting after a disconnect or error (default 5s)
- `ble.data_timeout_secs` — watchdog failsafe: if range notifications stop
  arriving for this long (default 5s), all toys are stopped and Fancypants
  reconnects (`0` disables)
- `ble.sample_interval_ms` / `ble.notify_interval_ms` — optional overrides
  (`10`–`5000` ms) written to the firmware's BLE config characteristic upon
  connecting (firmware defaults to `50` ms = 20Hz)
- `mapping.mode` — what drives the intensity (see [Mapping modes](#mapping-modes))
- `mapping.invert = true` — closer = more intense (default, distance mode only)
- `mapping.min_range_mm` / `max_range_mm` — active distance window (default `30`–`300` mm)
- `mapping.min_intensity` / `max_intensity` — output intensity span (`0.0`–`1.0`)
- `mapping.deadzone_mm` — distances beyond this produce zero intensity (`0` disables)
- `mapping.smoothing` — EMA smoothing factor (`0.0` = raw, `1.0` = max; `0.3` default)
- `buttplug.mode` — `"intiface"` (connect to Intiface Central over WebSocket,
  default) or `"standalone"` (also accepts `"embedded"`; run an in-process
  Buttplug BLE server so Intiface Central is not required)
- `buttplug.server_address` — Intiface WebSocket URL when `mode = "intiface"`
  (default `"ws://127.0.0.1:12345"`)
- `buttplug.device_index` / `buttplug.actuator_types` — single-toy selection and
  actuator configuration when `buttplug.devices` is not used (see below)
- `buttplug.devices` — optional `[[buttplug.devices]]` list for controlling
  multiple toys simultaneously, each with its own actuator and mapping settings
  (see [Controlling multiple toys](#controlling-multiple-toys))

## Mapping modes

`mapping.mode` selects what the intensity follows:

- **`"distance"`** (or **`"proximity"`**, default) — intensity follows the
  current distance reading, mapped across the `min_range_mm`–`max_range_mm`
  window. With `invert = true` closer means more intense; with `invert = false`
  further means more intense.
- **`"velocity"`** (or **`"speed"`**) — intensity follows how *fast* the
  distance is changing, regardless of direction: wave or move quickly for more
  intensity, hold still for none. `max_speed_mm_s` sets the speed that maps to
  full intensity (default 500; slow hand movement is roughly 100-300mm/s, a fast
  wave ~1000mm/s). `invert` and the range window are ignored in this mode.

In both modes, readings beyond `deadzone_mm` produce zero intensity, and
`smoothing` applies to the result. In velocity mode a little extra smoothing
(0.4-0.6) helps, since speed estimates are inherently jumpier than distance
readings.

## Per-actuator configuration

Toys can have multiple actuators of different types (e.g. a vibrator plus an
oscillating arm). `buttplug.actuator_types` lists which types to drive; each
entry is either a plain type name or a table with per-actuator settings:

```toml
[buttplug]
server_address = "ws://127.0.0.1:12345"
actuator_types = [
    # Full-range vibration following the sensor
    "Vibrate",
    # Oscillation capped at 60%, pulsing on/off twice a second while active
    { type = "Oscillate", min_intensity = 0.2, max_intensity = 0.6, pattern = "pulse", pulse_hz = 2.0 },
]
```

Supported types: `Vibrate`, `Rotate`, `Oscillate`, `Constrict`, `Inflate`,
`Position`. Settings per actuator:

| Setting         | Default    | Meaning                                                        |
|-----------------|------------|----------------------------------------------------------------|
| `min_intensity` | 0.0        | Output when the mapped intensity is just above zero            |
| `max_intensity` | 1.0        | Output when the mapped intensity is 1.0                        |
| `pattern`       | `constant` | `constant` follows the sensor; `pulse` gates it on/off         |
| `pulse_hz`      | 1.0        | Pulse frequency (only with `pattern = "pulse"`, max 10)        |

Actuators on the toy whose type isn't listed are left off. If a toy has
several actuators of the same type, the nth entry of that type configures its
nth actuator, and extra actuators reuse the last entry of that type.

## Controlling multiple toys

To drive several toys at once — each with its own actuator setup and mapping
mode — define one or more `[[buttplug.devices]]` blocks. Each block can match a
toy by `device_index` or case-insensitive substring `device_name` (or omit both
to claim the next available matching toy), and can optionally override any
field from `[mapping]` under `[buttplug.devices.mapping]`:

```toml
[buttplug]
server_address = "ws://127.0.0.1:12345"

# Toy 1: driven by proximity (closer = more intense, inherited from [mapping])
[[buttplug.devices]]
device_name = "Lush"
actuator_types = ["Vibrate"]

# Toy 2: driven by movement speed, with its own smoothing and pulse pattern
[[buttplug.devices]]
device_name = "Melt"
actuator_types = [
    { type = "Oscillate", max_intensity = 0.7, pattern = "pulse", pulse_hz = 2.0 },
]
[buttplug.devices.mapping]
mode = "velocity"
max_speed_mm_s = 600.0
smoothing = 0.5
```

Any mapping field omitted in `[buttplug.devices.mapping]` inherits the default
from the top-level `[mapping]` section.

## Run

### GUI Application

1. Power on the Feather (USB or battery) and turn on your toy(s).
2. If using Intiface mode, start Intiface Central; if using Standalone BLE mode,
   leave Intiface closed so Fancypants can connect to the toy directly.
3. Launch the app and click **Start**:

```bash
# Build (if needed) and launch via Make (uses `nix run` automatically on NixOS)
make run-app

# Or run the built binary directly
./build/app/fancypants-app
```

### Headless CLI

```bash
# Build (if needed) and launch via Make (uses `nix run .#cli` on NixOS)
make run-cli

# Or run the binary directly (auto-discovers ./config.toml or ~/.config/fancypants/config.toml)
./build/middleware/fancypants

# Point at a specific config with debug logging
./build/middleware/fancypants -c myconfig.toml -l debug

# Write a fresh commented config file and exit
./build/middleware/fancypants --generate-config
```

## What happens

1. Fancypants scans BLE for the `"Fancypants"` sensor peripheral.
2. Connects to the sensor, applies `sample_interval_ms` / `notify_interval_ms`
   overrides if configured, reads/subscribes to the Battery Service (`0x180F`),
   and subscribes to range notifications.
3. Connects to Intiface Engine over WebSocket (`mode = "intiface"`) or starts
   the in-process Buttplug BLE server and scans for toys (`mode = "standalone"`).
4. Matches each configured target toy (`[[buttplug.devices]]` or single-toy
   defaults) and plans its scalar actuators.
5. Maps incoming distance readings (`~20Hz`) through each toy's `RangeMapper`
   (distance or velocity + EMA smoothing), evaluates time-based `pulse`
   patterns, and sends `ScalarCmd` updates whenever actuator values change.
6. If sensor notifications stop for `ble.data_timeout_secs` or a connection
   drops, all toys are immediately stopped and Fancypants enters its
   reconnection loop.

```
[INFO] Found device: Fancypants
[INFO] Connected to Intiface Engine at ws://127.0.0.1:12345
[INFO] Target #1 matched device: We-Vibe Melt 2 (index 0) -> [PlannedActuator { index: 0, ... }]
[INFO] Sensor battery level: 88%
[INFO] Running — move your hand near the sensor!
```
