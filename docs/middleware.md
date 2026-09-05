# Middleware (fancypants)

## Getting the binary

Either download the `fancypants-<version>-linux-x86_64.tar.gz` tarball from
the [Releases page](../../../releases) (binary + sample `config.toml`), or
build from source (see [building.md](building.md)) — the binary then lands in
`build/middleware/fancypants`.

## Prerequisites (runtime only)

- Linux (the only platform built and tested so far)
- Intiface Central running (https://intiface.com/#intiface-central)
- BlueZ

On Arch:

```bash
sudo pacman -S bluez bluez-utils
sudo systemctl enable --now bluetooth
```

## Configure

The middleware looks for its config in this order:

1. The path given with `-c PATH` (must exist)
2. `./config.toml` (current directory)
3. `$XDG_CONFIG_HOME/fancypants/config.toml` (usually `~/.config/fancypants/config.toml`)

If none is found, it writes a default config to
`~/.config/fancypants/config.toml`, prints where it put it, and runs with
those defaults — so on first run you get a commented config file to edit
without any extra steps. (The release tarball also ships a sample
`config.toml` next to the binary.)

`--generate-config` does the same thing explicitly (to `-c PATH` if given);
it refuses to overwrite an existing file.

```bash
# Edit as needed
$EDITOR ~/.config/fancypants/config.toml
```

Key settings:

- `mapping.mode` — what drives the intensity (see [Mapping modes](#mapping-modes))
- `mapping.invert = true` — closer = more intense (default, distance mode only)
- `mapping.min_range_mm` / `max_range_mm` — active zone
- `mapping.deadzone_mm` — pull away past this to turn off
- `mapping.smoothing` — 0.3 is a good default, increase for smoother response
- `ble.data_timeout_secs` — failsafe: if range data stops for this long, the toy
  is stopped and the middleware reconnects (0 disables)
- `buttplug.actuator_types` — which actuators to drive, with optional
  per-actuator settings (see below)

## Mapping modes

`mapping.mode` selects what the intensity follows:

- **`"distance"`** (default) — intensity follows the current reading, mapped
  across the `min_range_mm`–`max_range_mm` window. With `invert = true`
  closer means more intense; with `invert = false` further means more
  intense.
- **`"velocity"`** — intensity follows how *fast* the distance is changing,
  regardless of direction: wave or move quickly for more intensity, hold
  still for none. `max_speed_mm_s` sets the speed that maps to full
  intensity (default 500; slow hand movement is roughly 100-300mm/s, a fast
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

## Run

1. Start Intiface Central and ensure your toy is connected
2. Power on the Feather (USB or battery)
3. Run the middleware:

```bash
# Config is found automatically (./config.toml, then ~/.config/fancypants/)
./build/middleware/fancypants

# Or point at a specific config; with debug logging:
./build/middleware/fancypants -c myconfig.toml -l debug
```

## What happens

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
