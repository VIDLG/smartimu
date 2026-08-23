# IMU Workspace 重构计划（已实施 → 已合并为 smartimu）

## Summary

当前架构已从 5 个独立 crate 合并为统一 `smartimu` crate + board app + 桌面 viewer：

```
crates/smartimu/
  src/
    bus.rs, types.rs, driver.rs, error.rs, protocol.rs, sample.rs, resource.rs
    drivers/       ← bmi270, hxy42688, icm42688, lsm6, qmi8658
    firmware/      ← device, runtime, transport, resources
    fusion/        ← Rust 传感器融合实现
    platform/      ← EspImuBus, EspDriverResources (feature-gated: esp)
apps/esp32c3-board/
tools/imu-viewer/  +  tools/imu-viewer-wgpu/
```

设计要点：

- `smartimu` 全程 `no_std`，`esp` feature 可选引入 `esp-hal`/`embassy-time`/`embedded-hal`
- 协议层"双表示、单语义"：`postcard + COBS + CRC32` 默认，`NDJSON` 调试
- 桌面 viewer 依赖 `smartimu`（不加 `esp`），不与嵌入式 HAL 冲突
- `From<SpiMode> for esp_hal::spi::Mode` 直接在 `platform/bus.rs` 实现，不再有孤儿规则摩擦

## Workspace 结构

```text
.
├─ Cargo.toml
├─ pixi.toml              ← host-side helper script environment
├─ justfile               ← common task entrypoints
├─ crates/
│  └─ smartimu/           ← 统一 crate（替代原 5 个独立 crate）
├─ apps/
│  └─ esp32c3-board/
├─ tools/
│  ├─ imu-viewer/
│  └─ imu-viewer-wgpu/
└─ scripts/
   └─ serial_open_check.py
```

## smartimu 模块设计

### 核心类型

- `ImuId { system_id: u16, sensor_id: u16 }`
- `ImuChip` — Unknown, Icm42688Hxy, Icm42688Pc, Bmi270, Qmi8658A, Sc7u22
- `ImuDescriptor`, `ImuSampleConfig`, `Quaternion`
- `BusId`, `ImuTargetId`, `SpiMode`, `SpiProfile`
- `RawSample`, `PhysicalSample`, `ScaleProfile`
- `ImuError`

### 核心 trait

- `ImuBus`
  - `apply_profile`, `read_reg`, `read_regs`, `write_reg`, `write_regs`, `delay_ms`
- `ImuDriver`
  - `chip`, `probe`, `reset`, `configure`, `read_raw`, `scale_profile`, `supported_sample_configs`
- `DriverResources`
  - `bytes(key) -> Option<&[u8]>` — 资源注入（如 BMI270 config blob）

### 设计原则

- 驱动层只面对 `ImuBus` + `ImuTargetId`，不感知平台
- `ImuBus` 暴露寄存器访问模型，不暴露 GPIO/SPI 外设实例
- 平台层 (`platform/`) 内部管理 GPIO 片选、SPI 配置，不泄漏到上层

## 协议层

`WireFrame` 枚举统一所有帧类型：

- `Hello`, `Topology`, `ProbeResult`, `Sample`, `Orientation`, `Error`, `Heartbeat`

编码方式：

- `feature = "binary"` → `postcard + COBS + CRC32`
- `feature = "json"` → JSON protocol encode/decode via `serde-json-core` (no_std)

## 当前实现状态

### 已完成

- ✅ `smartimu` 单 crate 合并完成
- ✅ `esp` feature gate 隔离平台依赖
- ✅ `apps/esp32c3-board` probe / init / sample loop / Json+Binary transport
- ✅ `tools/imu-viewer` + `tools/imu-viewer-wgpu` 双 viewer
- ✅ 录制、JSONL 导出、CSV 导出、回放

### 待增强

- viewer 3D 交互完善
- binary transport 实机稳定联调
- slot3/BMI270 识别问题

## 测试与验收

### 编译检查

- `cargo check -p smartimu` ✓
- `cargo check -p smartimu --features esp`（需 chip feature，由 board 层提供）
- `cargo check -p imu-viewer` ✓
- `cargo check -p imu-viewer-wgpu` ✓
- `cargo check -p esp32c3-board --target riscv32imc-unknown-none-elf`（需 ESP 工具链）

### 联调验收顺序

1. JSON 模式确认为默认 `Hello/Topology/Sample/Heartbeat`
2. Binary 模式确认 COBS/CRC 正确
3. 录制回放验证

### 当前联调结论

- slot1, slot2, slot4(QMI8658A), slot5 正常
- slot3/BMI270 未识别
- JSON 模式已实机联调，Binary 模式已构建验证
