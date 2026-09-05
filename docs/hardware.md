# Hardware

## Parts

- **Adafruit Feather nRF52840 Express**
- **Adafruit VL53L0X Time-of-Flight sensor breakout** (I2C, 30-1200mm range)
- **LiPo battery** (optional, JST-PH connector on Feather)
- Any Buttplug-compatible toy + Intiface Central

## Wiring

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

## Wearing it

See [wearable-design.md](../wearable-design.md) for the belt-mounted layout,
battery choice, and wiring run. Printable enclosures (OpenSCAD source +
exported STLs) live in [enclosures/](../enclosures/).
