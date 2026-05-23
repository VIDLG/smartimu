# IMU Workspace 重构计划

## Summary

本次重构目标是把当前单 crate 的 IMU 测试工程，彻底重构为通用 crate + board-specific app 的 workspace：

- `imu-core`
  - `no_std` 领域层，负责 IMU 抽象、数据模型、驱动接口、线协议类型
- `imu-drivers`
  - `no_std` 驱动层，负责具体 IMU 寄存器驱动实现与驱动注册表
- `imu-firmware`
  - 通用固件层，负责 probe、采样、transport、设备内运行时与板级抽象
- `imu-platform-esp`
  - Espressif 平台适配层，负责 `esp-hal`、SPI/UART/GPIO/clock/task 绑定，当前先落地 `esp32c3`
- `imu-viewer`
  - host 侧桌面 GUI，负责串口接收、协议解析、2D/3D 实时可视化、录制导出与回放

这是一次彻底重构，不保留旧 API、旧目录结构、旧文本串口输出格式。

协议层采用“双表示、单语义”设计：

- 正式链路默认使用 `postcard + COBS + CRC32`
- 调试链路支持 `NDJSON`
- 两种格式共享同一套 `WireFrame` 语义模型

## Workspace 结构

根目录改为 workspace，成员固定如下：

```text
.
├─ Cargo.toml
├─ crates/
│  ├─ imu-core/
│  ├─ imu-drivers/
│  ├─ imu-firmware/
│  └─ imu-platform-esp/
├─ apps/
│  └─ esp32c3-board/
└─ tools/
   └─ imu-viewer/
```

根 `Cargo.toml` 只保留：

- `[workspace]`
- `[workspace.package]`
- `[workspace.dependencies]`
- 统一 `profile`

当前根目录 `src/` 下的实现迁移完成后删除。  
当前 `build.rs` 中 BMI270 配置提取逻辑迁移到 `crates/imu-platform-esp` 或具体 board firmware crate，取决于最终配置资源归属。

## `imu-core` 设计

`imu-core` 只保留跨平台、与 HAL 无关、可复用的核心领域模型和协议定义。

### 核心类型

- `ImuId { system_id: u16, sensor_id: u16 }`
- `ImuKind`
- `ImuDescriptor`
- `BusId`
- `RawSample { accel: [i16; 3], gyro: [i16; 3], temp: Option<i16> }`
- `PhysicalSample { accel_g: [f32; 3], gyro_dps: [f32; 3], temp_c: Option<f32> }`
- `ScaleProfile`
- `ImuError`
- `BusProfile`
- `ImuConfig`
- `ImuCapabilities`
- `WireFormat`

### 核心 trait

- `ImuBus`
  - `apply_profile`
  - `read_reg`
  - `read_regs`
  - `write_reg`
  - `write_regs`
  - `delay_ms`
- `ImuDriver`
  - `kind`
  - `probe`
  - `reset`
  - `configure`
  - `read_raw`
  - `scale_profile`
  - `capabilities`

### 核心 trait 设计原则

最终抽象形式固定为：

- `ImuTargetId`
  - 表示当前总线上的一个逻辑目标设备
- `ImuBus`
  - 以 `target` 为参数执行寄存器访问
- `ImuDriver`
  - 所有生命周期方法都通过 `bus + target` 工作

设计原则：

- 驱动层只面对 `ImuBus` 和一个目标设备句柄
- 目标设备选择、总线仲裁、设备切换全部由 `ImuBus` 实现层负责
- 平台层可以内部使用 GPIO 控制、mux、shared bus lock 等机制，但不泄漏到 `imu-core`
- `imu-core` 不暴露 GPIO、SPI 外设实例或任何平台专属对象

### 建议接口草案

`ImuBus` 侧至少应支持：

- `apply_profile(target, profile)`
- `read_reg(target, reg, dummy_bytes)`
- `read_regs(target, reg, dummy_bytes, buf)`
- `write_reg(target, reg, value)`
- `write_regs(target, reg, data)`
- `delay_ms(ms)`

`ImuDriver` 侧至少应支持：

- `kind() -> ImuKind`
- `probe(bus, target) -> Result<bool, ImuError>`
- `reset(bus, target) -> Result<(), ImuError>`
- `configure(bus, target, config, resources) -> Result<(), ImuError>`
- `read_raw(bus, target) -> Result<RawSample, ImuError>`
- `scale_profile() -> ScaleProfile`
- `capabilities() -> ImuCapabilities`

设计约束：

- `probe()` 返回 `Result<bool, ImuError>`
  - `Ok(true)` 表示识别成功
  - `Ok(false)` 表示该 driver 不匹配当前设备
  - `Err(_)` 表示通信或执行错误
- `configure()` 是驱动完成进入流式采样前全部初始化的统一入口
- `ImuBus` 暴露的是寄存器访问模型，不暴露 GPIO、SPI 外设实例或平台类型

## 其余详细字段、协议与 viewer 设计

当前详细方案以代码实现和本文档后续补充为准。已确认落地的关键设计包括：

- `system_id / bus_id / sensor_id / ImuTargetId` 四层语义分离
- `BusId` 放在 topology/device 模型，不放协议 header
- `timestamp_us` 作为 sample 的设备侧源时间
- `status_bits` 使用 bitflags 语义
- BMI270 作为资源型驱动，通过 `DriverResources` 注入 blob
- viewer 基于 `ImuId` 归组，支持 2D dashboard 与 3D 展示

## 当前实现状态

当前仓库已经完成的主线实现：

- workspace 已建立
- `imu-core / imu-drivers / imu-firmware / imu-platform-esp` 已拆分
- `apps/esp32c3-board` 可构建，并具备：
  - probe
  - init
  - sample loop
  - `Json/Binary` transport 切换
- `tools/imu-viewer` 已具备：
  - `Auto / Json / Binary` 输入模式
  - topology 展示
  - accel / gyro 曲线
  - 基础 3D 姿态线框预览
  - 录制、JSONL 导出、CSV 导出、回放

当前仍待继续增强的部分：

- viewer 的更完整 3D 表现和交互
- device 侧将人类日志进一步收缩到最小
- binary transport 的实机联调与默认策略确认
- `slot3 / BMI270` 当前为已知未解决项，暂不阻塞其余 4 路 IMU 与主链路验收

## 测试与验收

### 当前可执行测试

- Host 侧基础检查
  - `cargo check -p imu-core -p imu-drivers -p imu-firmware -p imu-viewer`
- 设备侧 JSON 模式检查
  - `cargo check -p esp32c3-board --target riscv32imc-unknown-none-elf`
- 设备侧 Binary 模式检查
  - `cargo check -p esp32c3-board --no-default-features --features binary-transport --target riscv32imc-unknown-none-elf`
- Viewer 启动检查
  - `cargo run -p imu-viewer`

### 联调验收顺序

1. 先联调 JSON 模式
   - `esp32c3-board` 使用默认 `json-transport`
   - `imu-viewer` 选择 `Auto` 或 `Json`
   - 验证 `Hello / Topology / Sample / Heartbeat` 是否可见
2. 再联调 Binary 模式
   - `esp32c3-board` 使用 `binary-transport`
   - `imu-viewer` 选择 `Auto` 或 `Binary`
   - 验证 COBS 分帧、CRC 校验和 sample 曲线是否正常
3. 最后验证录制与回放
   - 录制 JSONL
   - 导出 CSV
   - 回放文件并确认曲线可复现

### 完成定义

以下条件同时满足时，可认为当前重构主线完成：

- `imu-core / imu-drivers / imu-firmware / imu-platform-esp / apps/esp32c3-board / imu-viewer` 全部可构建
- `esp32c3-board` 能在 JSON 和 Binary 两种模式下输出结构化数据
- `imu-viewer` 能在 `Auto / Json / Binary` 三种输入模式下稳定工作
- viewer 能展示 topology、2D 曲线、基础 3D 姿态预览
- viewer 能录制、导出、回放
- 在当前板子上，除 `slot3/BMI270` 外，其余 IMU 均可稳定识别与采样

## 当前联调结论

基于当前实机测试：

- `slot1` 正常
- `slot2` 正常
- `slot4` 已修复并正常识别为 `Qmi8658A`
- `slot5` 正常
- `slot3/BMI270` 仍未识别，当前表现为无响应

因此当前主链路的真实状态是：

- 4/5 路 IMU 已可用
- `JSON` 模式已完成实机联调
- `Binary` 模式已完成构建验证，仍待更稳定的实机验证

## 后续收尾项

### P0

- 真机联调 JSON 模式并确认 5 个 IMU 全部可见
- 真机联调 Binary 模式并确认 viewer 自动识别正常
- 清理设备侧非必要文本日志，仅保留 panic/fatal

### P1

- 为各驱动补 fake bus 单元测试
- 为 viewer 增加录制文件回归样本
- 将 transport 配置收口为更明确的 board config 或 feature 说明

### P2

- 提升 viewer 的 3D 表现
- 增加更完整的 session 统计
- 补充开发者文档和测试文档
