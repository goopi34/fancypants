# Troubleshooting

**"No Bluetooth adapters found"**
- `sudo systemctl start bluetooth`
- Check `rfkill list` for blocked adapters

**"Scan timeout: 'Fancypants' not found"**
- Is the Feather powered and running? Check USB serial console
- Is another device already connected to it? (nRF52840 supports 1 connection)
- Try `bluetoothctl` → `scan on` to verify the device is advertising

**"No Buttplug devices found"**
- **Intiface mode (`mode = "intiface"`):** Open Intiface Central, make sure its
  engine is started, your toy is connected there, and the WebSocket URL matches
  `buttplug.server_address` in `config.toml` (default `ws://127.0.0.1:12345`).
- **Standalone BLE mode (`mode = "standalone"`):** Make sure your toy is turned
  on and in BLE advertising/pairing mode, and that Intiface Central (or another
  phone/PC app) is **not** already connected to the toy — BLE toys only accept
  one central connection at a time.
- If using `device_name` or `device_index` filters in `[[buttplug.devices]]`,
  check that the substring or index matches your toy and that the toy supports
  at least one of the configured `actuator_types`.

**Android: BLE scan fails or disconnects when the screen turns off**
- Ensure **Nearby devices** (`BLUETOOTH_SCAN` and `BLUETOOTH_CONNECT`, or
  Location on Android 11 and below) and **Notifications** permissions are
  granted in Android Settings → Apps → Fancypants.
- Fancypants runs a foreground service (`Fancypants Active` notification) to
  keep BLE active while locked. If your device vendor aggressively kills
  background apps, set Battery usage for Fancypants to **Unrestricted**.

**Linux GUI: blank / empty window on launch**
- Some Wayland compositors and GPU drivers have issues with WebKitGTK's
  DMA-BUF renderer. `make run-app` and the Nix flake wrapper set
  `WEBKIT_DISABLE_DMABUF_RENDERER=1` automatically; if launching the binary
  directly, run:
  ```bash
  WEBKIT_DISABLE_DMABUF_RENDERER=1 ./build/app/fancypants-app
  ```

**Sensor stuck at 0mm or max range constantly**
- Check I2C wiring (SDA/SCL not swapped?)
- VL53L0X breakout getting 3.3V power?
- "Out of range" / no target reads are clamped by the firmware to
  `max_range_mm` (1200 by default), so a constant max reading usually
  just means nothing is in front of the sensor

**Jerky/stuttery toy response**
- Increase `mapping.smoothing` (try 0.5-0.7)
- Set `ble.notify_interval_ms` in `config.toml` (e.g. `100` for 10Hz) to reduce BLE notification frequency
- In `mode = "velocity"`, some jitter is expected — speed estimates amplify
  sensor noise, so lean on smoothing more than in distance mode
