# Fancypants Build System
# =======================
# Containerized builds for both firmware (nRF Connect SDK) and middleware (Rust).
# No local toolchains required — just docker/podman and make.
#
# Usage:
#   make firmware        Build nRF52 firmware (outputs build/firmware/zephyr.uf2)
#   make middleware       Build Rust middleware (outputs build/middleware/fancypants)
#   make all             Build both
#   make clean           Remove build artifacts
#   make shell-fw        Drop into firmware build container shell
#   make shell-mw        Drop into middleware build container shell
#   make flash            Print flashing instructions
#
# Configuration:
#   BOARD       nRF board target (default: adafruit_feather_nrf52840)
#   NCS_TAG     nRF Connect SDK version tag (default: v2.9-branch)
#   CONTAINER   Container runtime: docker or podman (default: auto-detect)

# ── Configuration ──────────────────────────────────────────────────────
BOARD          ?= adafruit_feather_nrf52840
NCS_TAG        ?= v2.9-branch
NCS_IMAGE      := nordicplayground/nrfconnect-sdk:$(NCS_TAG)
RUST_IMAGE     := rust:1-bookworm

PROJECT_DIR    := $(shell pwd)
BUILD_DIR      := $(PROJECT_DIR)/build
FW_BUILD_DIR   := $(BUILD_DIR)/firmware
MW_BUILD_DIR   := $(BUILD_DIR)/middleware

# Auto-detect container runtime
CONTAINER ?= $(shell command -v podman 2>/dev/null && echo podman || echo docker)

# UID/GID forwarding so build artifacts aren't owned by root
USER_ARGS := -u $(shell id -u):$(shell id -g)

# ── Targets ────────────────────────────────────────────────────────────
.PHONY: all firmware middleware clean shell-fw shell-mw flash help

all: firmware middleware

# ── Firmware ───────────────────────────────────────────────────────────
firmware: $(FW_BUILD_DIR)/zephyr.uf2

$(FW_BUILD_DIR)/zephyr.uf2: firmware/src/*.c firmware/src/*.h firmware/prj.conf firmware/Kconfig firmware/CMakeLists.txt firmware/boards/*.overlay
	@echo "══════════════════════════════════════════════════════════════"
	@echo "  Building fancypants-nrf52 firmware"
	@echo "  Board:  $(BOARD)"
	@echo "  NCS:    $(NCS_TAG)"
	@echo "  Image:  $(NCS_IMAGE)"
	@echo "══════════════════════════════════════════════════════════════"
	@mkdir -p $(FW_BUILD_DIR)
	$(CONTAINER) run --rm \
		-v $(PROJECT_DIR)/firmware:/workdir/project/firmware:ro \
		-v $(FW_BUILD_DIR):/workdir/project/build \
		-w /workdir/project/firmware \
		$(NCS_IMAGE) \
		west build -p always -b $(BOARD) --build-dir /workdir/project/build
	@$(CONTAINER) run --rm \
		-v $(FW_BUILD_DIR):/fix \
		$(NCS_IMAGE) \
		chown -R $(shell id -u):$(shell id -g) /fix
	@echo ""
	@echo "✓ Firmware built: $(FW_BUILD_DIR)/zephyr/zephyr.uf2"
	@# Copy UF2 to top-level build dir for convenience
	@cp $(FW_BUILD_DIR)/zephyr/zephyr.uf2 $(FW_BUILD_DIR)/zephyr.uf2 2>/dev/null || \
		echo "  (UF2 not generated — check if board target supports UF2 output)"

# ── Middleware ─────────────────────────────────────────────────────────
middleware: $(MW_BUILD_DIR)/fancypants

$(MW_BUILD_DIR)/fancypants: middleware/src/*.rs middleware/Cargo.toml
	@echo "══════════════════════════════════════════════════════════════"
	@echo "  Building fancypants middleware"
	@echo "  Image:  $(RUST_IMAGE)"
	@echo "══════════════════════════════════════════════════════════════"
	@mkdir -p $(MW_BUILD_DIR) $(BUILD_DIR)/cargo-cache
	$(CONTAINER) run --rm \
		-e HOST_UID=$(shell id -u) \
		-e HOST_GID=$(shell id -g) \
		-v $(PROJECT_DIR)/middleware:/workdir/middleware:ro \
		-v $(MW_BUILD_DIR):/workdir/output \
		-v $(BUILD_DIR)/cargo-cache:/usr/local/cargo/registry \
		-w /workdir \
		$(RUST_IMAGE) \
		sh -c '\
			cp -r middleware /tmp/build && \
			cd /tmp/build && \
			apt-get update -qq && \
			apt-get install -y -qq libdbus-1-dev pkg-config libudev-dev >/dev/null 2>&1 && \
			cargo build --release 2>&1 && \
			cp target/release/fancypants /workdir/output/fancypants && \
			chown $$HOST_UID:$$HOST_GID /workdir/output/fancypants \
		'
	@echo ""
	@echo "✓ Middleware built: $(MW_BUILD_DIR)/fancypants"

# ── Interactive Shells ─────────────────────────────────────────────────
shell-fw:
	@echo "Entering firmware build container (NCS $(NCS_TAG))..."
	$(CONTAINER) run --rm -it \
		-v $(PROJECT_DIR)/firmware:/workdir/project/firmware \
		-v $(FW_BUILD_DIR):/workdir/project/build \
		-w /workdir/project/firmware \
		$(NCS_IMAGE) \
		/bin/bash

shell-mw:
	@echo "Entering middleware build container..."
	$(CONTAINER) run --rm -it \
		$(USER_ARGS) \
		-v $(PROJECT_DIR)/middleware:/workdir/middleware \
		-v $(MW_BUILD_DIR):/workdir/output \
		-v $(BUILD_DIR)/cargo-cache:/usr/local/cargo/registry \
		-w /workdir/middleware \
		$(RUST_IMAGE) \
		/bin/bash

# ── Flash Instructions ─────────────────────────────────────────────────
flash:
	@echo ""
	@echo "Flashing fancypants-nrf52 firmware via UF2:"
	@echo "  1. Connect Feather nRF52840 to USB"
	@echo "  2. Double-tap the reset button"
	@echo "  3. Wait for FTHR840BOOT drive to appear"
	@echo "  4. Run:"
	@echo ""
	@echo '     cp $(FW_BUILD_DIR)/zephyr.uf2 /run/media/$$USER/FTHR840BOOT/'
	@echo ""
	@echo "  The board will auto-reboot. Verify with:"
	@echo "     picocom /dev/ttyACM0"
	@echo ""

# ── Clean ──────────────────────────────────────────────────────────────
clean:
	rm -rf $(BUILD_DIR)

# ── Help ───────────────────────────────────────────────────────────────
help:
	@echo "Fancypants Build System"
	@echo ""
	@echo "Targets:"
	@echo "  make firmware        Build nRF52 firmware (UF2)"
	@echo "  make middleware       Build Rust middleware binary"
	@echo "  make all             Build both (default)"
	@echo "  make clean           Remove all build artifacts"
	@echo "  make shell-fw        Interactive firmware build shell"
	@echo "  make shell-mw        Interactive middleware build shell"
	@echo "  make flash            Show flashing instructions"
	@echo ""
	@echo "Options:"
	@echo "  BOARD=<target>       Board target (default: $(BOARD))"
	@echo "  NCS_TAG=<tag>        NCS version (default: $(NCS_TAG))"
	@echo "  CONTAINER=<runtime>  docker or podman (default: auto-detect)"
	@echo ""
	@echo "Examples:"
	@echo "  make firmware BOARD=adafruit_feather_nrf52840/nrf52840/uf2"
	@echo "  make firmware NCS_TAG=v2.7-branch"
	@echo "  make CONTAINER=podman"
	