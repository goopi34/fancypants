# Fancypants Wearable Design Spec

## Overview

The fancypants hardware is worn as a belt-mounted system. The VL53L0X
rangefinder sits above the crotch facing outward, measuring the distance
to a partner's hand (or body). The Feather nRF52840 rides on the
hip opposite the belt clasp, powered by a 500mAh LiPo battery plugged directly into
the Feather's JST-PH connector.

## Layout

```
                  ┌─────────────┐
                  │  VL53L0X    │  ← sensor, facing outward
                  │  rangefinder│
                  └──────┬──────┘
                         │ I2C (4-wire: VIN, GND, SDA, SCL)
    ┌────────────────────┼────────────────────┐
    │                    │         BELT        │
    │  ┌──────────┐      │              ┌─────┐│
    │  │ Feather  │◄─────┘              │CLASP││
    │  │ nRF52840 │                     └─────┘│
    │  │ + LiPo   │                            │
    │  └──────────┘                            │
    └──────────────────────────────────────────┘
```

## Component Placement

### Belt

Standard belt or wide elastic waistband. The clasp sits on the right
hip (user's perspective, adjustable to preference). The belt carries
two components and the wiring between them.

### VL53L0X Sensor (front center)

- Mounted at belt line, centered above the crotch
- Sensor window faces directly outward (perpendicular to body)
- Field of view is 25° cone — aim straight out, not angled down
- Secured to belt with a small 3D-printed or sewn pouch
- Keep the sensor window unobstructed

### Feather nRF52840 + Battery (left hip)

- Mounted on the hip opposite the clasp
- Secured with a belt-mounted pouch, pocket, or velcro holster
- The BLE antenna is internal to the Feather — no external antenna
  needed, and the hip position gives decent range in all directions

## Power

The Feather is powered by an Adafruit 500mAh 3.7V LiPo battery
(product 1578, ~$8) plugged into the onboard JST-PH connector.

- Runtime: ~15 hours at typical draw (~30mA average with BLE +
  sensor polling)
- The battery and Feather are compact enough to share a single
  belt pouch
- The Feather's onboard charger handles recharging — just plug
  USB-C into the Feather. The yellow CHG LED lights during charge,
  goes off when complete. Charge time is ~5 hours from dead.
- When USB is connected, the Feather hot-swaps to USB power and
  charges the LiPo simultaneously

**Buy from Adafruit** (or verify JST-PH polarity matches Adafruit
convention). Cheap batteries from Amazon frequently have reversed
polarity and will destroy the Feather's charging circuit.

Link: https://www.adafruit.com/product/1578

## Wiring Run

The only wiring that runs along the belt is the I2C connection between
the Feather (left hip) and the VL53L0X (front center). This is 4 wires:

| Wire  | Color (STEMMA QT convention) | Notes              |
|-------|------------------------------|--------------------|
| VIN   | Red                          | 3.3V from Feather  |
| GND   | Black                        | Ground              |
| SDA   | Blue                         | I2C data            |
| SCL   | Yellow                       | I2C clock           |

Wire run is approximately 20-30cm depending on waist size. Options:

- **STEMMA QT cable**: Easiest. Adafruit sells 200mm and 300mm lengths.
  Plug directly into the VL53L0X breakout. Solder or use a STEMMA QT
  adapter on the Feather end.
- **Ribbon cable**: Low profile, can be sewn flat against the belt
  interior. 4-conductor ribbon or individual silicone wires.
- **Sewn channels**: For a cleaner look, sew a fabric channel along
  the belt interior and thread the wires through it.

I2C is tolerant of this wire length at 100kHz (the default). No
additional pullups or buffering needed beyond what the VL53L0X breakout
provides.

## Future Improvements

- 3D-printed enclosure that clips directly to belt
