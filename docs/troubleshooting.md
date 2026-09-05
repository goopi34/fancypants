# Troubleshooting

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

**Sensor stuck at 0mm or max range constantly**
- Check I2C wiring (SDA/SCL not swapped?)
- VL53L0X breakout getting 3.3V power?
- "Out of range" / no target reads are clamped by the firmware to
  `max_range_mm` (1200 by default), so a constant max reading usually
  just means nothing is in front of the sensor

**Jerky/stuttery toy response**
- Increase `mapping.smoothing` (try 0.5-0.7)
- Increase `notify_interval_ms` in firmware config characteristic
- In `mode = "velocity"`, some jitter is expected — speed estimates amplify
  sensor noise, so lean on smoothing more than in distance mode
