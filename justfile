set shell := ["sh", "-cu"]

serial_port := env("ESPFLASH_PORT", "")
espflash_version := "4.5.0"

# Pixi provides the environment; this file owns project workflows.

# List available recipes
_default:
    @just --list

# Install the ESP32-C3 Rust target into the Pixi-managed toolchain
rust-target-install:
    python scripts/install_rust_target.py

# Verify that the ESP32-C3 Rust target is installed
rust-target-check:
    python scripts/install_rust_target.py --check

# Check host-side packages that build for the native platform
check-host:
    cargo check -p smartimu -p imu-viewer -p imu-viewer-wgpu -p smartimu-hil

# Check ESP32-C3 firmware with the default JSON transport
check-device: rust-target-install
    cargo check -p esp32c3-board --target riscv32imc-unknown-none-elf

# Check ESP32-C3 firmware with binary transport
check-device-binary: rust-target-install
    cargo check -p esp32c3-board --no-default-features --features binary-transport --target riscv32imc-unknown-none-elf

# Check both host-side packages and ESP32-C3 firmware
check: check-host check-device

# Build host-side packages
build-host:
    cargo build -p smartimu -p imu-viewer -p imu-viewer-wgpu -p smartimu-hil

# Build ESP32-C3 firmware with the default JSON transport
build-device: rust-target-install
    cargo build -p esp32c3-board --target riscv32imc-unknown-none-elf

# Build ESP32-C3 firmware with binary transport
build-device-binary: rust-target-install
    cargo build -p esp32c3-board --no-default-features --features binary-transport --target riscv32imc-unknown-none-elf

# Build host-side packages and ESP32-C3 firmware
build: build-host build-device

# Run host-side tests
test-host:
    cargo test -p smartimu -p imu-viewer -p imu-viewer-wgpu -p smartimu-hil

# Run the ignored hardware-in-the-loop test against a connected JSON firmware board
hil port=serial_port seconds="10":
    SMARTIMU_HIL_PORT="{{ port }}" SMARTIMU_HIL_SECONDS="{{ seconds }}" cargo test -p smartimu-hil --test esp32c3_board -- --ignored --nocapture

# Flash and monitor ESP32-C3 firmware through cargo runner
run-device: rust-target-install
    cargo run -p esp32c3-board --target riscv32imc-unknown-none-elf

# Install the pinned espflash version with the Pixi-managed Rust toolchain
espflash-install:
    cargo install espflash --locked --version {{ espflash_version }} --registry crates-io

# Flash firmware; auto-detect the port unless a parameter or ESPFLASH_PORT is provided
flash-device port=serial_port:
    python -m scripts.flash_device "{{ port }}" target/riscv32imc-unknown-none-elf/debug/esp32c3-board

# Launch the egui desktop viewer
viewer:
    cargo run -p imu-viewer

# Launch the wgpu desktop viewer
viewer-wgpu:
    cargo run -p imu-viewer-wgpu

# Run rustfmt on all workspace packages
fmt:
    cargo fmt --all

# Check rustfmt without modifying files
fmt-check:
    cargo fmt --all --check

# Run clippy on host-side packages
clippy-host:
    cargo clippy -p smartimu -p imu-viewer -p imu-viewer-wgpu -p smartimu-hil -- -D warnings

# Run clippy on the HIL harness without linting its dependencies
clippy-hil:
    cargo clippy -p smartimu-hil --no-deps -- -D warnings

# Run clippy on ESP32-C3 firmware
clippy-device: rust-target-install
    cargo clippy -p esp32c3-board --target riscv32imc-unknown-none-elf -- -D warnings

# Run clippy on host-side packages and ESP32-C3 firmware
clippy: clippy-host clippy-device

# Check a serial port; auto-detect it unless a parameter or ESPFLASH_PORT is provided
serial-open-check port=serial_port baud="115200":
    python scripts/serial_open_check.py "{{ port }}" "{{ baud }}"

# Install local git hooks managed by lefthook
hooks-install:
    lefthook install

# Remove Cargo build outputs
clean:
    cargo clean

# Print the Pixi-managed tool versions and verify the ESP32-C3 target
doctor: rust-target-install
    rustc --version
    cargo --version
    python scripts/install_rust_target.py --check
    pixi --version
    just --version
