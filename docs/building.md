# Building

Everything builds in containers — no local toolchains needed. Just `docker`
(or `podman`) and `make`.

```bash
# Build everything
make

# Build just firmware
make firmware

# Build just middleware CLI
make middleware

# Build the Tauri v2 GUI desktop application
make app

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
├── cargo-cache/             ← persistent Rust dependency cache
└── cargo-target/            ← persistent Rust build cache
```

## Build options

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

## Tests and lint

```bash
make test        # middleware test suite
make lint        # rustfmt + clippy, clang-format
make coverage    # middleware coverage report (build/coverage/html/)
```

## Cutting a release

Pushing a `v*` tag triggers the release workflow
([.github/workflows/release.yml](../.github/workflows/release.yml)), which
runs the tests, builds both components, and publishes a GitHub Release with:

- `fancypants-nrf52-<version>.uf2` — firmware, ready to drag onto the
  `FTHR840BOOT` drive
- `fancypants-<version>-linux-x86_64.tar.gz` — Linux CLI + Tauri v2 GUI
  application, sample `config.toml`, and docs
- `fancypants-<version>-windows-x86_64.zip` — Windows CLI + Tauri v2 GUI
  application, sample `config.toml`, and docs
- `fancypants-<version>-android-arm64.apk` — sideloadable Android APK

```bash
git tag v0.2.0
git push origin v0.2.0
```

The tag (minus the `v`) becomes the version embedded in both binaries.

## Manual builds (without containers)

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
