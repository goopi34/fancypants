# Hardware

## Parts

- **Adafruit Feather nRF52840 Express**
- **Adafruit VL53L0X Time-of-Flight sensor breakout** (I2C address `0x29`, 30–1200mm range)
- **LiPo battery** (optional, e.g. Adafruit 500mAh 3.7V LiPo, JST-PH connector on Feather)
- Any **Buttplug-compatible toy(s)** (controlled either directly over BLE in standalone mode or via Intiface Central)

## Wiring

If your VL53L0X has a STEMMA QT / Qwiic connector, just use the cable.
Otherwise wire to the Feather's default I2C0 pins (`SDA` = `P0.12`, `SCL` = `P0.11`):

| Feather | VL53L0X |
|---------|---------|
| 3V      | VIN     |
| GND     | GND     |
| SDA     | SDA     |
| SCL     | SCL     |

The Adafruit VL53L0X breakout includes 10K I2C pullups and a voltage regulator,
so no additional external components are needed. Battery voltage is monitored
automatically via the Feather's onboard `VBAT / 2` divider on `P0.29` (`AIN5`).

## Wearing it

See [wearable-design.md](../wearable-design.md) for the belt-mounted layout,
battery choice, and wiring run. Printable enclosures (OpenSCAD source +
exported STLs) live in [enclosures/](../enclosures/).
