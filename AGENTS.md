# AGENTS.md

This file defines project-specific instructions for coding agents working in this repository.

## Project context

This is an embedded Rust project for a 5-in-1 IMU test board running on ESP32-C3. The codebase is `no_std`-first, with host-side tools and tests where possible.

Primary architecture:

```text
apps/esp32c3-board      board-specific firmware
crates/smartimu         shared protocol, drivers, fusion, bus traits
tools/imu-viewer        host-side viewer
```

All IMU hardware access must go through the `ImuBus` abstraction. The five IMUs share one SPI bus and use separate chip-select pins, so only one target may be selected at a time.

## Testing conventions

- Keep tests deterministic and runnable on the host whenever possible.
- Prefer colocated unit tests for module-private behavior.
- When adding module-level unit tests, keep test code in separate `*_tests.rs` files and include them from the source module with `#[cfg(test)]` and `#[path = "..."] mod tests;`.
- Use `crates/*/tests/` integration tests for public API behavior, protocol compatibility, golden vectors, and cross-module behavior.
- Do not place large test modules inline in production source files unless the test is very small.
- Do not mock the driver being tested. IMU driver tests should use the real driver with a fake `ImuBus` and chip-specific fake IMU model.
- Fake IMU tests should verify protocol-relevant behavior: probe IDs, revision checks, SPI profile fallback, initialization write order, data-ready status, sample layout, endian handling, and error paths.
- Fusion tests should feed deterministic accelerometer/gyroscope samples directly and assert numerical invariants with explicit tolerances.
- Protocol tests should cover binary round-trip, malformed packets, CRC failures, packet length limits, JSON feature behavior where relevant, and golden compatibility cases when wire compatibility matters.
- Hardware-in-the-loop tests should be minimal, clearly documented, and explicitly marked as requiring ESP32-C3 hardware.

## Test file layout

Use this style for internal module tests:

```rust
// src/protocol.rs

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
```

```text
crates/smartimu/src/protocol.rs
crates/smartimu/src/protocol_tests.rs
crates/smartimu/src/fusion/mod.rs
crates/smartimu/src/fusion/mod_tests.rs
```

Use integration tests for public APIs and wire compatibility:

```text
crates/smartimu/tests/protocol_binary.rs
crates/smartimu/tests/protocol_json.rs
crates/smartimu/tests/protocol_golden.rs
```

Shared test support should be clearly marked and gated:

```text
crates/smartimu/src/test_support/
crates/smartimu/tests/support/
```

Prefer `#[cfg(test)]` for crate-internal support. Use a dedicated `test-support` feature only if integration tests or host tools must reuse the fake devices.

## Fake IMU testing model

Use two layers:

1. A generic fake bus that implements `ImuBus` and records bus operations.
2. A chip-specific fake IMU model that simulates the register protocol required by one real chip.

The intended shape is:

```text
real driver + FakeImuBus + FakeIcm42688Pc/FakeQmi8658/... model
```

The fake chip should model only behavior the driver depends on. Start small with register reads/writes and grow into a state machine only when testing sequence-sensitive behavior.

## Documentation

- Put long-form testing strategy in `docs/testing.md`.
- Keep this file focused on rules agents should follow while editing the repository.
- Update `docs/test-plan.md` when changing release or hardware validation procedures.
