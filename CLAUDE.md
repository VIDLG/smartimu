# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is an embedded Rust project for a 5-in-1 IMU test board running on ESP32-C3 (RISC-V). The board tests 5 different IMU sensors from various manufacturers by probing them via SPI and continuously streaming their 6-axis data (accelerometer + gyroscope).

**Supported IMU Sensors:**
- ICM-42688-HXY (华轩阳电子) - Slot 1
- ICM-42688-PC (Tokmas) - Slot 2
- BMI270 (Bosch) - Slot 3
- QMI8658A (QST) - Slot 4
- SC7U22 (SILAN) - Slot 5

## Build and Flash Commands

**Flash and monitor (default):**
```bash
cargo run     # or: cargo r
```
This compiles, flashes via USB (espflash), and opens serial monitor to view output.

**Build only:**
```bash
cargo build
```

**Run tests:**
```bash
cargo test --test hello_test
```

**Alternative runner (JTAG debugger):**
To use probe-rs with a JTAG debugger instead of espflash, edit `.cargo/config.toml` and change the runner line to:
```toml
runner = "probe-rs run --chip=esp32c3 --preverify --always-print-stacktrace --no-location --catch-hardfault"
```

## Architecture Overview

### Hardware Constraints

All 5 IMU sensors share the same SPI bus (MOSI/MISO/SCK) but have individual CS (chip select) pins. **Only one sensor can be communicated with at a time** - violating this causes bus conflicts. All CS pins default to HIGH (deselected).

**ESP32-C3 Pin Assignment:**
- SPI SCK: GPIO6
- SPI MOSI: GPIO7
- SPI MISO: GPIO2
- CS pins: GPIO1, GPIO3, GPIO4, GPIO5, GPIO10 (for slots 1-5 respectively)

### Software Architecture

The codebase uses a **layered architecture with runtime driver detection**:

```
main.rs (application entry)
    ↓
app.rs (slot orchestration)
    ↓
board.rs (slot configuration with candidate drivers)
    ↓
bus.rs (SPI bus abstraction with profile switching)
    ↓
imu/driver.rs (driver trait interface)
    ↓
drivers/*.rs (individual IMU driver implementations)
```

#### Key Architectural Concepts

1. **Slot-Based Configuration (board.rs)**
   - Each physical slot has an expected IMU type and a list of candidate drivers
   - Candidates specify which driver to try and which bus profiles (SPI modes) to attempt
   - Example: Slot 1 tries hxy42688 driver with Mode 0/3, then lsm6 with Mode 3, etc.

2. **Bus Profile System (bus.rs)**
   - A `BusProfile` encapsulates SPI mode (Mode 0-3) and frequency
   - The bus can dynamically switch between profiles via `apply_profile()`
   - This allows testing sensors that may respond to different SPI modes
   - The `ImuBus` trait provides a hardware-agnostic interface for driver operations

3. **Driver Detection Flow (app.rs)**
   - For each slot, iterate through candidates
   - For each candidate, try each bus profile
   - Call `probe()` to check if sensor responds correctly
   - Call `init()` to initialize the detected sensor
   - Stop on first successful detection and use that driver for streaming

4. **Driver Interface (imu/driver.rs)**
   - `DriverOps` trait: `probe()`, `init()`, `read_raw()`, `scale()`, `kind()`, `label()`
   - Drivers return `RawSample` (raw register values)
   - `RawSample` is converted to `PhysicalSample` (physical units: g, dps) using the driver's `ScaleProfile`

5. **Fusion and BMI270**
   - Fusion is implemented in Rust under `crates/smartimu/src/fusion`.
   - BMI270 support and its external configuration blob have been removed from the active codebase.

### Module Breakdown

- **crates/imu-core/**: Core IMU models, traits, and protocol types
- **crates/imu-drivers/**: Concrete driver implementations
- **crates/imu-firmware/**: Platform-independent runtime and device topology
- **crates/imu-platform-esp/**: legacy platform split; current ESP integration lives under `smartimu`
- **apps/esp32c3-board/**: Current board-specific device application entry point
- **tools/imu-viewer/**: Host-side desktop viewer scaffold

## Development Workflow

**When adding a new IMU driver:**

1. Create `crates/imu-drivers/src/your_sensor.rs`
2. Implement the `ImuDriver` trait
3. Define a public static `DRIVER: YourDriver` instance
4. Add to `crates/imu-drivers/src/lib.rs`
5. Add candidate selection in the current device profile
6. If the sensor needs special initialization (like BMI270's config blob), handle it in the owning platform crate build script

**When modifying SPI communication:**

- All SPI operations go through `ImuBus` trait methods
- `write_regs()` sends data to a register address
- `read_regs()` reads data from a register address (handles read bit and dummy bytes)
- Always ensure CS pin is managed correctly (LOW during transfer, HIGH otherwise)
- Bus profile switches are transparent to drivers - the app layer handles profile cycling

**When debugging sensor issues:**

- Check `print_probe_snapshot()` output - shows raw register reads across different modes
- Verify the expected WHO_AM_I values in `spi.rs` match sensor datasheets
- Try different bus profiles if probe fails - some sensors are mode-sensitive
- Confirm GPIO pin assignments match actual PCB wiring

## Testing Conventions

See `docs/testing.md` for the full testing strategy.

- Prefer host-runnable deterministic tests whenever possible.
- For module-level unit tests, keep test code in separate `*_tests.rs` files and include them with `#[cfg(test)]` plus `#[path = "..."] mod tests;`.
- Use integration tests under `crates/*/tests/` for public API behavior, protocol compatibility, golden vectors, and cross-module behavior.
- IMU driver tests should use the real driver with a fake `ImuBus` and chip-specific fake IMU models. Do not mock the driver being tested.
- Fake IMU tests should cover probe IDs, revision checks, SPI profile fallback, initialization write order, data-ready behavior, sample layout, endian handling, and error paths.
- Fusion tests should feed deterministic accel/gyro samples directly and assert numerical invariants with explicit tolerances.
- Protocol tests should cover binary round-trip, malformed packets, CRC failures, packet length limits, JSON feature behavior where relevant, and golden compatibility cases when wire compatibility matters.
- Hardware-in-the-loop tests should be minimal and clearly marked as requiring ESP32-C3 hardware.

## Important Notes

- This project uses `esp-hal` 1.0+ which has significant API changes from 0.x versions
- Logging uses `defmt` + `esp-println` with `espflash --monitor` for decoding
- The project uses `no_std` - no standard library, embedded environment
- Embassy provides async runtime (`embassy-executor`, `embassy-time`)
- Tests use `embedded-test` with `harness = false` configuration
- DEFMT_LOG level is set to "off" in `.cargo/config.toml` - change to "debug" or "trace" for verbose logging

## Troubleshooting Sensor Detection

If sensors show as "unavailable" after running `cargo r`, check `docs/troubleshooting.md` for detailed diagnostic guide.

**Quick checks:**
1. Probe output showing all `0xFF` = sensor not soldered or CS pin issue
2. Probe output showing unexpected fixed value = SPI timing/frequency issue
3. "PROBE OK" but "init failed" = power supply or configuration issue
4. Detection succeeds but data shows "NA" = data ready check too strict or ODR misconfiguration

**Common fixes:**
- Lower the configured SPI frequency in the current board/device profile
- Increase power-up delay in the board firmware startup flow
- Verify hardware connections match GPIO pin assignments
- Check sensor datasheets for correct WHO_AM_I register addresses and SPI modes

## Chinese Documentation

The primary documentation ([README.md](README.md), [docs/project-guide.md](docs/project-guide.md)) is in Chinese as this project targets Chinese IMU manufacturers and the LCSC electronics marketplace. Key Chinese terms:
- 供应商 = vendor/manufacturer
- 芯片选择 = chip select (CS)
- 六轴 = 6-axis (3-axis accel + 3-axis gyro)
- 加速度计 = accelerometer
- 陀螺仪 = gyroscope
