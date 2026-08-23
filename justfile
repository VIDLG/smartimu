set shell := ["sh", "-cu"]

esp_target := "riscv32imc-unknown-none-elf"
board_pkg := "esp32c3-board"
viewer_pkg := "imu-viewer"
serial_port := env("ESPFLASH_PORT", "COM15")

# List available recipes
_default:
    @just --list

# Check host-side packages that build for the native platform
check-host:
    cargo check -p smartimu -p imu-viewer -p imu-viewer-wgpu

# Check ESP32-C3 firmware with the default JSON transport
check-device:
    cargo check -p {{ board_pkg }} --target {{ esp_target }}

# Check ESP32-C3 firmware with binary transport
check-device-binary:
    cargo check -p {{ board_pkg }} --no-default-features --features binary-transport --target {{ esp_target }}

# Check both host-side packages and ESP32-C3 firmware
check: check-host check-device

# Build ESP32-C3 firmware with the default JSON transport
build-device:
    cargo build -p {{ board_pkg }} --target {{ esp_target }}

# Build ESP32-C3 firmware with binary transport
build-device-binary:
    cargo build -p {{ board_pkg }} --no-default-features --features binary-transport --target {{ esp_target }}

# Flash and monitor ESP32-C3 firmware through cargo runner
run-device:
    cargo run -p {{ board_pkg }}

# Flash already-built default firmware with espflash; override port with ESPFLASH_PORT
flash-device:
    espflash flash --chip esp32c3 --port {{ serial_port }} target/{{ esp_target }}/debug/{{ board_pkg }}

# Launch the egui desktop viewer
viewer:
    cargo run -p {{ viewer_pkg }}

# Run rustfmt on all workspace packages
fmt:
    cargo fmt --all

# Check rustfmt without modifying files
fmt-check:
    cargo fmt --all --check

# Run clippy on host-side packages
clippy-host:
    cargo clippy -p smartimu -p imu-viewer -p imu-viewer-wgpu -- -D warnings

# Run clippy on ESP32-C3 firmware
clippy-device:
    cargo clippy -p {{ board_pkg }} --target {{ esp_target }} -- -D warnings

# Run clippy on host-side packages and ESP32-C3 firmware
clippy: clippy-host clippy-device

# Check whether a serial port can be opened; override port with `just serial-open-check COM16`
serial-open-check port=serial_port baud="115200":
    pixi run serial-open-check {{ port }} {{ baud }}

# Install local git hooks managed by lefthook
hooks-install:
    pixi run hooks-install

# Remove Cargo build outputs
clean:
    cargo clean

# Print installed Rust targets and key tool versions
doctor:
    rustc --version
    cargo --version
    rustup target list --installed
    just --version
    pixi --version
