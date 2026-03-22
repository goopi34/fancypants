# Fancypants Build System
# =======================
# Containerized builds for both firmware (nRF Connect SDK) and middleware (Rust).
# No local toolchains required — just docker/podman and make.
#
# Usage:
#   make firmware        Build nRF52 firmware (outputs build/firmware/zephyr.uf2)
#   make middleware       Build Rust middleware (outputs build/middleware/fancypants)
#   make all             Build both
#   make lint            Lint both components (inside containers)
#   make coverage        Run coverage report (HTML output in build/coverage/)
#   make test            Run middleware tests (inside container)
#   make format-middleware Auto-format middleware Rust sources (run once to establish baseline)
#   make format-firmware Auto-format firmware C/H sources (run once to establish baseline)
#   make clean           Remove build artifacts
#   make shell-fw        Drop into firmware build container shell
#   make shell-mw        Drop into middleware build container shell
#   make flash            Print flashing instructions
#
# Configuration:
#   BOARD       nRF board target (default: adafruit_feather_nrf52840)
#   NCS_TAG     nRF Connect SDK version tag (default: v2.9-branch)
#   CONTAINER   Container runtime: docker or podman (default: auto-detect)
#   VERSION     Application version string (default: from git describe)

# ── Configuration ──────────────────────────────────────────────────────
BOARD          ?= adafruit_feather_nrf52840
NCS_TAG        ?= v2.9-branch
NCS_IMAGE      := nordicplayground/nrfconnect-sdk:$(NCS_TAG)
RUST_IMAGE     := rust:1-bookworm
CLANG_IMAGE    := ubuntu:24.04

PROJECT_DIR    := $(shell pwd)
BUILD_DIR      := $(PROJECT_DIR)/build
FW_BUILD_DIR   := $(BUILD_DIR)/firmware
MW_BUILD_DIR   := $(BUILD_DIR)/middleware

# Auto-detect container runtime
CONTAINER ?= $(shell command -v podman >/dev/null 2>&1 && echo podman || echo docker)

# Application version: strip leading v from tag, or use branch+sha, or "dev"
VERSION ?= $(shell git describe --tags --always --dirty 2>/dev/null | sed 's/^v//' || echo dev)

# UID/GID forwarding so build artifacts aren't owned by root
USER_ARGS := -u $(shell id -u):$(shell id -g)

# ── Targets ────────────────────────────────────────────────────────────
.PHONY: all firmware middleware lint lint-middleware lint-firmware test test-middleware coverage format-middleware format-firmware clean shell-fw shell-mw flash help

all: firmware middleware

# ── Firmware ───────────────────────────────────────────────────────────
firmware: $(FW_BUILD_DIR)/zephyr.uf2

$(FW_BUILD_DIR)/zephyr.uf2: firmware/src/*.c firmware/src/*.h firmware/prj.conf firmware/Kconfig firmware/CMakeLists.txt firmware/boards/*.overlay
	@echo "══════════════════════════════════════════════════════════════"
	@echo "  Building fancypants-nrf52 firmware"
	@echo "  Board:   $(BOARD)"
	@echo "  NCS:     $(NCS_TAG)"
	@echo "  Image:   $(NCS_IMAGE)"
	@echo "  Version: $(VERSION)"
	@echo "══════════════════════════════════════════════════════════════"
	@mkdir -p $(FW_BUILD_DIR)
	$(CONTAINER) run --rm \
		-v $(PROJECT_DIR)/firmware:/workdir/project/firmware:ro \
		-v $(FW_BUILD_DIR):/workdir/project/build \
		-w /workdir/project/firmware \
		$(NCS_IMAGE) \
		west build -p always -b $(BOARD) --build-dir /workdir/project/build -- -DAPP_VERSION_STRING=$(VERSION)
	@$(CONTAINER) run --rm \
		-v $(FW_BUILD_DIR):/fix \
		$(NCS_IMAGE) \
		chown -R $(shell id -u):$(shell id -g) /fix
	@echo ""
	@UF2=$$(find $(FW_BUILD_DIR) -name "zephyr.uf2" | head -1) && \
		[ -n "$$UF2" ] || { echo "ERROR: zephyr.uf2 not found under $(FW_BUILD_DIR)"; exit 1; } && \
		cp "$$UF2" $(FW_BUILD_DIR)/zephyr.uf2
	@echo "✓ Firmware built: $(FW_BUILD_DIR)/zephyr.uf2"

# ── Middleware ─────────────────────────────────────────────────────────
middleware: $(MW_BUILD_DIR)/fancypants

$(MW_BUILD_DIR)/fancypants: middleware/src/*.rs middleware/Cargo.toml middleware/build.rs
	@echo "══════════════════════════════════════════════════════════════"
	@echo "  Building fancypants middleware"
	@echo "  Image:   $(RUST_IMAGE)"
	@echo "  Version: $(VERSION)"
	@echo "══════════════════════════════════════════════════════════════"
	@mkdir -p $(MW_BUILD_DIR) $(BUILD_DIR)/cargo-cache
	$(CONTAINER) run --rm \
		-e HOST_UID=$(shell id -u) \
		-e HOST_GID=$(shell id -g) \
		-e FANCYPANTS_VERSION=$(VERSION) \
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
			FANCYPANTS_VERSION=$$FANCYPANTS_VERSION cargo build --release 2>&1 && \
			cp target/release/fancypants /workdir/output/fancypants && \
			chown $$HOST_UID:$$HOST_GID /workdir/output/fancypants \
		'
	@echo ""
	@echo "✓ Middleware built: $(MW_BUILD_DIR)/fancypants"

# ── Lint ───────────────────────────────────────────────────────────────
lint: lint-middleware lint-firmware

lint-middleware:
	@echo "══════════════════════════════════════════════════════════════"
	@echo "  Linting middleware (fmt + clippy)"
	@echo "══════════════════════════════════════════════════════════════"
	@mkdir -p $(BUILD_DIR)/cargo-cache
	$(CONTAINER) run --rm \
		-v $(PROJECT_DIR)/middleware:/workdir/middleware:ro \
		-v $(BUILD_DIR)/cargo-cache:/usr/local/cargo/registry \
		-w /workdir \
		$(RUST_IMAGE) \
		sh -c '\
			cp -r middleware /tmp/build && \
			cd /tmp/build && \
			apt-get update -qq && \
			apt-get install -y -qq libdbus-1-dev pkg-config libudev-dev >/dev/null 2>&1 && \
			rustup component add rustfmt clippy 2>/dev/null && \
			cargo fmt --check && \
			cargo clippy -- -D warnings \
		'

lint-firmware:
	@echo "══════════════════════════════════════════════════════════════"
	@echo "  Linting firmware (clang-format)"
	@echo "══════════════════════════════════════════════════════════════"
	$(CONTAINER) run --rm \
		-v $(PROJECT_DIR):/workdir:ro \
		-w /workdir \
		$(CLANG_IMAGE) \
		sh -c '\
			apt-get update -qq && \
			apt-get install -y -qq clang-format >/dev/null 2>&1 && \
			find firmware/src -name "*.c" -o -name "*.h" | xargs clang-format --dry-run --Werror \
		'

# ── Test ───────────────────────────────────────────────────────────────
test: test-middleware

test-middleware:
	@echo "══════════════════════════════════════════════════════════════"
	@echo "  Testing middleware"
	@echo "══════════════════════════════════════════════════════════════"
	@mkdir -p $(BUILD_DIR)/cargo-cache
	$(CONTAINER) run --rm \
		-v $(PROJECT_DIR)/middleware:/workdir/middleware:ro \
		-v $(BUILD_DIR)/cargo-cache:/usr/local/cargo/registry \
		-w /workdir \
		$(RUST_IMAGE) \
		sh -c '\
			cp -r middleware /tmp/build && \
			cd /tmp/build && \
			apt-get update -qq && \
			apt-get install -y -qq libdbus-1-dev pkg-config libudev-dev >/dev/null 2>&1 && \
			cargo test \
		'

# ── Coverage ───────────────────────────────────────────────────────────
coverage:
	@echo "══════════════════════════════════════════════════════════════"
	@echo "  Middleware coverage (llvm-cov)"
	@echo "══════════════════════════════════════════════════════════════"
	@mkdir -p $(BUILD_DIR)/coverage $(BUILD_DIR)/cargo-cache
	$(CONTAINER) run --rm \
		-e HOST_UID=$(shell id -u) \
		-e HOST_GID=$(shell id -g) \
		-v $(PROJECT_DIR)/middleware:/workdir/middleware:ro \
		-v $(BUILD_DIR)/cargo-cache:/usr/local/cargo/registry \
		-v $(BUILD_DIR)/coverage:/workdir/coverage-out \
		-w /workdir \
		$(RUST_IMAGE) \
		sh -c '\
			cp -r middleware /tmp/build && \
			cd /tmp/build && \
			apt-get update -qq && \
			apt-get install -y -qq libdbus-1-dev pkg-config libudev-dev >/dev/null 2>&1 && \
			rustup component add llvm-tools 2>/dev/null && \
			cargo install cargo-llvm-cov --quiet 2>&1 | tail -1 && \
			cargo llvm-cov \
				--html --output-dir /workdir/coverage-out \
				--fail-under-lines 1 \
				2>&1 && \
			chown -R $$HOST_UID:$$HOST_GID /workdir/coverage-out \
		'
	@echo ""
	@echo "✓ Coverage report: $(BUILD_DIR)/coverage/index.html"

# ── Format (establish baseline) ────────────────────────────────────────
format-middleware:
	@echo "Auto-formatting middleware Rust sources with cargo fmt..."
	$(CONTAINER) run --rm \
		$(USER_ARGS) \
		-v $(PROJECT_DIR)/middleware:/workdir/middleware \
		-v $(BUILD_DIR)/cargo-cache:/usr/local/cargo/registry \
		-w /workdir/middleware \
		$(RUST_IMAGE) \
		sh -c 'rustup component add rustfmt 2>/dev/null && cargo fmt'
	@echo "✓ Middleware sources formatted"

format-firmware:
	@echo "Auto-formatting firmware C/H sources with clang-format..."
	$(CONTAINER) run --rm \
		-v $(PROJECT_DIR):/workdir \
		-w /workdir \
		$(CLANG_IMAGE) \
		sh -c '\
			apt-get update -qq && \
			apt-get install -y -qq clang-format >/dev/null 2>&1 && \
			find firmware/src -name "*.c" -o -name "*.h" | xargs clang-format -i \
		'
	@echo "✓ Firmware sources formatted"

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
	@echo "  make lint            Lint both components inside containers"
	@echo "  make lint-middleware  Run cargo fmt --check and clippy"
	@echo "  make lint-firmware    Run clang-format --dry-run on firmware sources"
	@echo "  make test            Run all tests inside containers"
	@echo "  make coverage        Run middleware coverage (HTML report in build/coverage/)"
	@echo "  make test-middleware  Run cargo test for middleware"
	@echo "  make format-middleware Auto-format middleware Rust sources (run once for baseline)"
	@echo "  make format-firmware  Auto-format firmware C/H sources (run once for baseline)"
	@echo "  make clean           Remove all build artifacts"
	@echo "  make shell-fw        Interactive firmware build shell"
	@echo "  make shell-mw        Interactive middleware build shell"
	@echo "  make flash            Show flashing instructions"
	@echo ""
	@echo "Options:"
	@echo "  BOARD=<target>       Board target (default: $(BOARD))"
	@echo "  NCS_TAG=<tag>        NCS version (default: $(NCS_TAG))"
	@echo "  CONTAINER=<runtime>  docker or podman (default: auto-detect)"
	@echo "  VERSION=<string>     Version to embed (default: from git describe)"
	@echo ""
	@echo "Examples:"
	@echo "  make firmware BOARD=adafruit_feather_nrf52840/nrf52840/uf2"
	@echo "  make firmware NCS_TAG=v2.7-branch"
	@echo "  make all VERSION=1.2.3"
	@echo "  make CONTAINER=podman"
