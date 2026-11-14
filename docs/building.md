# Building

Everything builds in containers — no local toolchains needed. Just `docker`
(or `podman`) and `make`.

```bash
# Build firmware + CLI middleware (default target)
make

# Build just firmware
make firmware

# Build just middleware CLI
make middleware

# Build the Tauri v2 GUI desktop application
make app

# Build (if needed) and launch the GUI application or CLI
# (automatically uses `nix run` on NixOS, or containerized build elsewhere)
make run-app
make run-cli

# Print UF2 flashing instructions
make flash

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
│   └── fancypants           ← headless CLI binary
├── app/
│   └── fancypants-app       ← Tauri v2 GUI desktop app
├── coverage/
│   └── html/index.html      ← HTML coverage report (from `make coverage`)
├── cargo-cache/             ← persistent Rust dependency cache
└── cargo-target/            ← persistent Rust build cache
```

## Build options

You can override the NCS version (default `v2.9-branch`), board target
(default `adafruit_feather_nrf52840`), embedded version string (default from
`git describe`), or container runtime (auto-detects `podman` or `docker`):

```bash
make firmware NCS_TAG=v2.9-branch
make firmware BOARD=adafruit_feather_nrf52840/nrf52840/uf2
make all VERSION=1.2.3
make CONTAINER=podman
```

For interactive debugging, drop into a build container shell:

```bash
make shell-fw    # firmware (NCS/Zephyr environment)
make shell-mw    # middleware (Rust environment)
```

## Tests, lint, and formatting

```bash
make test               # middleware unit test suite
make lint               # lint-middleware + lint-app (rustfmt + clippy) + lint-firmware (clang-format)
make coverage           # middleware llvm-cov HTML report (enforces >= 80% line coverage)
make format-middleware  # auto-format middleware and app/src-tauri Rust sources with cargo fmt
make format-firmware    # auto-format firmware C/H sources with clang-format
```

Pull requests run `.github/workflows/ci.yml`, which audits GitHub Actions
workflows with `zizmor`, runs `make lint` and `make coverage`, and verifies
builds across all target platforms (firmware `.uf2`, Linux `.deb`/`.rpm`/`.AppImage`,
Windows `.exe` CLI + GUI, and Android `arm64` `.apk`).

## Cutting a release

Pushing a `v*` tag triggers the release workflow
([.github/workflows/release.yml](../.github/workflows/release.yml)), which
runs the tests, builds both components, and publishes a GitHub Release with:

- `fancypants-nrf52-<version>.uf2` — firmware, ready to drag onto the
  `FTHR840BOOT` drive
- `fancypants-<version>-linux-x86_64.tar.gz` — Linux CLI + Tauri v2 GUI
  application, sample `config.toml`, and docs
- `fancypants-<version>-linux-amd64.deb` — Debian/Ubuntu package for the
  Tauri v2 GUI application
- `fancypants-<version>-linux-x86_64.rpm` — Fedora/RHEL/openSUSE RPM package
  for the Tauri v2 GUI application
- `fancypants-<version>-linux-x86_64.AppImage` — Portable Linux AppImage for
  the Tauri v2 GUI application
- `fancypants-<version>-windows-x86_64.zip` — Windows CLI + Tauri v2 GUI
  application, sample `config.toml`, and docs
- `fancypants-<version>-android-arm64.apk` — sideloadable Android APK

```bash
git tag v0.2.0
git push origin v0.2.0
```

The tag (minus the `v`) becomes the version embedded in both binaries.

## Manual builds (without containers)

### Nix / NixOS (`flake.nix`, `x86_64-linux` & `aarch64-linux`)

```bash
# Launch the Tauri v2 GUI application directly (default flake app)
nix run
# or: nix run .#app

# Launch the headless CLI
nix run .#cli

# Build packages into ./result
nix build .#app
nix build .#cli

# Enter a development shell with Rust, cargo-tauri, WebKitGTK, clang-tools, openscad, and zizmor
nix develop
```

### Firmware (nRF Connect SDK / `west`)

```bash
cd firmware
west build -b adafruit_feather_nrf52840
```

### Middleware CLI & Desktop GUI App (Linux / Windows)

On Debian/Ubuntu, install the required system libraries first:

```bash
# CLI dependencies
sudo apt-get install libdbus-1-dev pkg-config libudev-dev

# Additional GUI dependencies (Tauri v2 / WebKitGTK)
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf desktop-file-utils
```

Then build with Cargo (works on both Linux and Windows):

```bash
# Headless CLI
cargo build --release --manifest-path middleware/Cargo.toml

# Tauri v2 GUI application binary
cargo build --release --manifest-path app/src-tauri/Cargo.toml

# Or package Linux .deb, .rpm, and .AppImage bundles via tauri-cli
cargo install tauri-cli --version "^2.0.0" --locked
cd app/src-tauri
NO_STRIP=true cargo tauri build --bundles deb,rpm,appimage
```

### Android APK (`aarch64-linux-android`)

Requires Android SDK (Platform 34, Build-Tools 34.0.0) and NDK r27
(`NDK_HOME` set):

```bash
rustup target add aarch64-linux-android
cargo install tauri-cli --version "^2.0.0" --locked

cd app/src-tauri
cargo tauri android init --ci
./mobile/setup-android.sh
cargo tauri android build --apk --target aarch64
```

`./mobile/setup-android.sh` patches the generated Android project (`gen/android`)
with BLE and foreground-service permissions, copies `MainActivity.kt`,
`FancypantsBleService.kt`, and vendored `droidplug` / `jni-utils` Java sources
so both the sensor client and embedded Buttplug BLE server can access Android's
`BluetoothAdapter` from Rust threads, and adds ProGuard keep rules.
