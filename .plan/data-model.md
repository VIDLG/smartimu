# SmartIMU 核心数据模型

> 状态：Draft
>
> 本文用于审查核心 ID、Topology、Bus、Driver、Sample、Fusion、Runtime 和 Actor 数据结构。宏观架构、用例、交互和状态机见 [architecture.md](architecture.md)；Host/Device 协议与 HostClient 见 [protocol.md](protocol.md)。

本节用于审查关键类型和字段。以下是接近 Rust 的架构伪代码，不代表最终 API 已冻结；字段宽度、容量和序列化 tag 在协议 v1 定稿前仍可调整。

## 0. 现有代码复用基线

目标架构优先复用 `crates/smartimu` 中已经有明确语义和测试基础的类型，不为同一概念另起一套名字。

| 处理方式 | 现有类型/组件 | 目标计划 |
|---|---|---|
| 直接复用 | `BusId`、`TimestampUs`、`SampleIndex` | 保留现有 newtype、宽度和辅助方法 |
| 直接复用 | `RangeG`、`RangeDps`、`SampleRateHz`、`ImuSampleConfig` | 保留现有配置模型 |
| 直接复用 | `RawImu6`、`RawTemperature`、`RawImuSample` | 保留现有 raw sample 模型；当前未实现的 Sensor timestamp 字段标为后续 |
| 直接复用 | `PhysicalImu6`、`PhysicalTemperature`、`PhysicalImuSample`、`Imu6Scale`、`ImuSampleScale`、`TemperatureScale` | 保留现有单位分型和转换逻辑 |
| 直接复用 | `Quaternion`、`ProtocolVersion`、`ImuIdentity`、`ImuChipModel` | 保留现有领域语义 |
| 直接复用 | `ImuBus` 的寄存器级访问和逻辑 target 思路 | 扩展 I²C 时保持同一边界，不让 driver 接触 GPIO/HAL；route/profile 合并为 `BusBinding` 后由 bus owner 原子应用 |
| 直接复用 | `DriverInfo`、`ProbeRegisterMatch`、`ProbeRegisterReadout`、`SampleRegisterReadout` | 保留 identity/readout 描述逻辑；将 probe 语义收敛为已知型号的 identity verification |
| 直接复用 | 当前 `FusionFilter`、`FusionFilterSettings`、`FusionConvention` | 保留现有 6-axis 算法实现；删除无效果的 `magnetic_rejection`，第二种算法出现前不扩建复杂 registry |
| 概念复用 | `SampleConfigCapability`、`ImuChipProfile` | 保留能力模型和校验方法；公共 owned 数据按项目约定使用 `Vec`，timestamp bool 后续按真实芯片信息细化 |
| 概念迁移 | `BoardImuConfig` | 保留 Sensor ID、逻辑 target、预期型号和静态 Board 定义；去掉 candidate drivers，补唯一 bus profile、默认 config 和 axis mapping，形成 `SensorDefinition` |
| 小幅迁移 | `SpiProfile` | 保留 `mode + frequency_khz`；删除当前未使用的 `id`，不加入推测字段 |
| 改名迁移 | `SystemId`、`SessionId` | 分别改为语义明确的 `DeviceId`、`BootSessionId` |
| 改名迁移 | `ImuId { system_id, sensor_id }` | Host 侧改为 `SensorRef`；Device-scoped payload 只使用 `SensorId` |
| 改名/扩展 | `ImuTargetId` + `SpiProfile` | 合并为支持 SPI/I²C 的 `BusBinding`，继续保持逻辑 route 不暴露 GPIO |
| 语义迁移 | `DetectedChipInfo`、`ImuDriver::probe` | 改为 `VerifiedIdentity`、`verify_identity`，不做 candidate driver discovery |
| 直接复用 | `ResponseResult<T>`、`ProtocolErrorCode` 与 `SmartImuError` 映射 | 保留成功/结构化错误语义，调整错误上下文字段 |
| 概念复用 | `HostRequest`、`DeviceResponse`、`DeviceEvent` 及 Ping/Inventory/Sampling/Sample/Orientation/Heartbeat payload | 保留 v1 仍需要的业务语义，移除重复 wrapper/header；Power 因无硬件 backend 标为后续，详见 `protocol.md` |
| Link 层复用 | `BinaryEncoder`/`BinaryDecoder` 的 postcard + CRC32 + COBS 和错误分类 | 迁到 UART adapter；ESP-NOW 不套 COBS，packet 大小按 Link 分开 |
| 协议迁移 | `MessageSeq`、当前 `Response/Event` | Host 请求改用 `RequestId`，Response 增加 `in_reply_to`；不保留 Device 全局消息序号 |
| 后续新增 | `AxisMapping`、`StreamId`、`LinkId`、Actor/runtime、calibration | 现有代码无等价模型，按对应章节分阶段加入 |

现有代码不合理的部分不会为了“复用”而保留：候选 driver/profile 动态猜测、固定 `SessionId(1)`、缺少 request correlation 和无效果的 Fusion `magnetic_rejection` 应按目标边界迁移。SPI adapter 当前固定 read/write mask 是已支持芯片下的实现假设，v1 不把它扩散成 `SpiProfile` 公共字段；出现不同寄存器命令格式的真实 driver 时，再把该规则下沉到 driver/read transaction。

## 1. 基础 ID 与时间类型

共享领域模型使用 `alloc::vec::Vec` 和 `alloc::string::String`，不把容量写进公共类型的 const generic。协议 decoder 在分配前检查 wire 上限；Actor mailbox/outbox 仍由 runtime 使用有界队列，避免内存失控。

所有 ID 使用 newtype，不直接在 API 中混用裸整数：

```rust
use alloc::{string::String, vec::Vec};

pub struct DeviceId(pub u64);
pub struct McuHardwareId(pub u64); // [后续：平台诊断] 仅用于派生 DeviceId 或受控诊断，不进入常规消息
pub struct DeviceModel(pub u16); // [后续：多产品型号] 使用稳定 wire tag，不表示设备实例
pub struct BootSessionId(pub u32);
pub struct BusId(pub u8); // Device topology 内部
pub struct SensorId(pub u16); // 直接复用现有类型宽度
pub struct RequestId(pub u32);
pub struct OperationId(pub u32); // [后续：异步长任务] 支持进度、取消、查询或跨 Link 跟踪时启用
pub struct StreamId(pub u32);
pub struct SampleIndex(pub u32);
pub struct ConfigRevision(pub u32); // [后续：Stream 内热配置] v1 通过新 StreamId 表达配置变化
pub struct FusionRevision(pub u32); // [后续：Stream 内热切换] v1 通过新 StreamId 表达算法或参数变化
pub struct EventSeq(pub u32); // [后续：可靠非采样 Event] 必须按订阅或 Link 定义作用域
pub struct TimestampUs(pub u64);
pub struct LinkId(pub u32); // Device 本次启动内单调分配的逻辑回程路由，不进入 wire protocol
```

建议的作用域：

| 类型 | 阶段 | 作用域与生成方 |
|---|---|---|
| `DeviceId` | v1 | SmartIMU Device 在 application protocol 中的稳定逻辑身份 |
| `McuHardwareId` | 后续 | MCU 芯片硬件身份，例如 ESP eFuse base MAC；仅用于派生或受控诊断 |
| `DeviceModel` | 后续 | 产品型号的稳定 tag，不表示某台设备实例 |
| `BootSessionId` | v1 | 一次 Device 启动；来自启动计数或随机 nonce |
| `BusId` | v1/internal | Device 内局部 bus ID |
| `SensorId` | v1 | Device 内局部 Sensor ID |
| `RequestId` | v1 | Host 生成，在一条 Host session 内关联 Command/Response |
| `OperationId` | 后续 | 异步长任务支持进度、取消、查询或跨 Link 跟踪时，由 Device 生成 |
| `StreamId` | v1 | Device 在 StartSampling 成功时生成，标识一次采样生命周期 |
| `SampleIndex` | v1 | 每个 Sensor、每个 Stream 独立递增，允许 wrapping |
| `ConfigRevision` | 后续 | 仅在允许一个 Stream 内热修改采样配置时启用 |
| `FusionRevision` | 后续 | 仅在允许一个 Stream 内热切换 Fusion algorithm/settings 时启用 |
| `EventSeq` | 后续 | 非采样 Event 需要可靠缺失检测时启用；不能使用 Device 全局序号制造跨 Link 假丢包 |
| `TimestampUs` | v1 | Device 单调时钟域中的微秒时间 |
| `LinkId` | v1/internal | Device 本次启动内由 Link Manager 单调分配的逻辑 Host endpoint/回程路由；不进入 wire protocol |

`LinkId` 解决 Actor 的内部回程路由：Command 进入 inbox 时附带来源 `LinkId`，Actor 输出 Response 时原样带回，Link Manager 据此发送到正确的 UART session 或 ESP-NOW peer。订阅和断开清理也按 `LinkId` 隔离。它不是 Device/Host 身份或 wire 字段。为避免延迟 Reply 被发送到复用后的新 endpoint，v1 在一次 boot 内单调分配 `u32`，关闭后不复用；重启后由新的 `BootSessionId` 开始新的内部作用域。

Host 引用 Sensor 时使用复合标识：

```rust
pub struct SensorRef { // [后续：smartimu-host convenience type] 不进入 Device-scoped wire payload
    pub device_id: DeviceId,
    pub sensor_id: SensorId,
}
```

Device-scoped wire payload 只携带 `SensorId`；Host 聚合多个 Device 时才构造 `SensorRef`，避免在同一 payload 中重复传递可能不一致的 Device ID。

## 2. Device 身份

```rust
pub struct ProtocolVersion {
    pub major: u8,
    pub minor: u8,
}

pub struct FirmwareVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

pub struct HardwareRevision {
    pub major: u8,
    pub minor: u8,
}

pub struct DeviceIdentity {
    pub device_id: DeviceId,
    pub mcu_hardware_id: Option<McuHardwareId>, // [后续：平台诊断]
    pub device_model: Option<DeviceModel>, // [后续：多产品型号]
    pub hardware_revision: HardwareRevision,
    pub firmware_version: FirmwareVersion,
    pub protocol_version: ProtocolVersion,
    // pub capabilities: DeviceCapabilities, // [后续：出现真实的 capability negotiation 需求时再加入]
}
```

`DeviceId` 标识整个 SmartIMU Device，不是 IMU Sensor ID，也不等同于裸 MCU ID。开发阶段可以默认由 ESP eFuse base MAC 稳定派生；量产阶段可以改用烧录的产品序列号映射，同时把 `McuHardwareId` 作为可选诊断信息。更换 MCU 后逻辑 Device 是否保持同一个 `DeviceId`，由产品身份策略决定。

`FirmwareVersion` 是固件发布版本，当前使用 SemVer 风格的 `major.minor.patch`。它用于展示、诊断和升级判断；wire 兼容性只由 `ProtocolVersion` 决定，不能根据固件版本猜测协议兼容性。

v1 不定义 `DeviceCapabilities` 或通用 `LinkCapabilities`。Host 先通过 `ProtocolVersion` 判断协议兼容性，通过 `GetTopology`/`GetSensorInfo` 获取实际 Sensor 与采样能力；未实现的可选 Command 返回结构化 `Unsupported`。UART 与 ESP-NOW 各自在 backend 中使用编译期消息长度上限，不做 capability negotiation。

`DeviceCapabilities` 留作后续扩展：只有出现同一协议版本下大量可选功能组合，而且 Host 确实需要在发命令前统一发现这些能力时才引入。届时优先使用普通结构、enum 和 `Vec`，不默认编码为 flags/bitset。

直接复用现有 `ImuChipModel` enum。只为已经接入 topology/driver registry 的型号定义 variant；不能因为数据手册中存在某型号就宣称固件支持。

WHO_AM_I 和芯片 revision 属于 Sensor identity signature：

```rust
pub enum ImuChipModel {
    Icm42688Hxy,
    Icm42688Pc,
    Qmi8658A,
    Sc7u22,
}

pub struct ImuIdentity {
    pub who_am_i: u8,
    pub revision: Option<u8>,
}
```

它不作为物理 Sensor 或 Device 的全局唯一 ID。

Power telemetry 不进入 v1 核心模型。当前 `docs/hardware.md` 没有电池、充电管理、电量计、VBUS 检测或低电量阈值定义；现有 `PowerStatus` 也只有协议构造器和 Viewer 展示，没有真实采集 backend。

现有结构还混淆了“当前主要供电源”和“电池是否存在”，并用 `Unknown` 与多个 `Option` 重复表达未知状态。因此不直接把这些类型同步进目标架构。后续出现真实电源硬件时，优先使用 `ActivePowerSource`、有最低有效测量要求的 `BatteryStatus`，并将低电量告警与原始状态分开设计。详细审查与后续草案见 [protocol.md](protocol.md#11-power-数据模型审查)。

## 3. Device Topology

开发板 5 IMU 和量产单 IMU 使用相同结构：

```rust
pub struct DeviceTopology {
    pub buses: Vec<BusDefinition>,
    pub sensors: Vec<SensorDefinition>,
}

pub struct BusDefinition {
    pub id: BusId,
    pub kind: BusKind,
}

pub enum BusKind {
    Spi,
    I2c,
}

pub struct SensorDefinition {
    pub id: SensorId,
    pub model: ImuChipModel, // 复用现有 enum；topology 中唯一选择对应 driver
    pub bus: BusBinding, // 总线路由和该 Sensor 的访问参数，避免 target/profile 类型不匹配
    pub default_config: ImuSampleConfig,
    pub axis_mapping: AxisMapping, // Sensor Frame 到统一 Board Frame 的固定安装方向
}
```

Board profile 提供静态 topology：

- 5 IMU 开发板：`sensors.len() == 5`。
- 量产设备：`sensors.len() == 1`。
- Sensor 列表顺序不作为身份，身份由 `SensorId` 决定。
- v1 不使用 `required: bool`：topology 中列出的 Sensor 都是预期存在的。全部可用为 `Ready`，部分可用为 `Degraded`，全部不可用才为 `Faulted`。以后真有可选插槽或主/备 Sensor 策略时，再引入有明确语义的 policy enum，而不是恢复一个含义模糊的 bool。

SPI 规范没有定义通用的“单次最多传输 N 字节”；只要 CS 和具体从设备协议允许，时钟可以继续。基础 I²C 规范也没有统一的 transaction payload 最大长度；SMBus block 等上层协议的长度限制不能当作普通 I²C 的限制。

实际长度上限通常来自 MCU/HAL、DMA descriptor、adapter scratch buffer、内存预算，或具体 Sensor 的寄存器/FIFO 协议。现有 ESP SPI adapter 的 `MAX_WRITE_BYTES = 40` 和 `MAX_READ_BYTES = 64` 只是当前实现的内部 buffer 大小，不是 SPI capability，也不应进入 topology、`BusBinding` 或 v1 `ImuBus` 公共接口。当前普通寄存器和 12-byte IMU sample 读取不需要查询最大传输量；backend 继续在内部拒绝越界请求，并返回明确错误。

`BusTransferLimits` 和 `ImuBus::transfer_limits()` 从 v1 计划删除。以后真正实现 FIFO/bulk transfer，并且 driver 确实需要根据不同 backend 动态分块时，再基于实际实现决定使用固定 chunk size 还是增加 runtime limit query，不提前承诺数据结构。

- `SensorId` 和 `BusId` 在各自 Device 内唯一。

## 4. Bus binding 与 profile

```rust
pub enum BusBinding {
    Spi {
        bus_id: BusId,
        chip_select_id: u8,
        profile: SpiProfile,
    },
    I2c {
        bus_id: BusId,
        address_7bit: u8,
        profile: I2cProfile,
    },
}

pub struct SpiProfile {
    pub mode: SpiMode,
    pub frequency_khz: u32,
}

pub struct I2cProfile {
    pub frequency_khz: u32,
}

pub enum SpiMode {
    Mode0,
    Mode1,
    Mode2,
    Mode3,
}

pub struct Turnaround(pub u8);

pub trait ImuBus {
    fn write_regs(
        &mut self,
        bus: &BusBinding,
        reg: u8,
        data: &[u8],
    ) -> Result<(), SmartImuError>;

    fn read_regs(
        &mut self,
        bus: &BusBinding,
        reg: u8,
        turnaround: Turnaround,
        data: &mut [u8],
    ) -> Result<(), SmartImuError>;

    fn write_reg(
        &mut self,
        bus: &BusBinding,
        reg: u8,
        value: u8,
    ) -> Result<(), SmartImuError>;

    fn read_reg(
        &mut self,
        bus: &BusBinding,
        reg: u8,
        turnaround: Turnaround,
    ) -> Result<u8, SmartImuError>;
}
```

`chip_select_id` 是 Board 逻辑 target，由 `smartimu-esp` 映射到 GPIO。Driver、protocol 和 Host 不看到 GPIO 编号。

`SpiProfile` 以现有实现为基线，只保留实际使用的 `mode` 和 `frequency_khz`：

- 当前 `id` 没有读取方，profile 作为 `Copy + Eq` 值即可比较和复用，因此目标模型删除 `id`。
- `read_mask`/`auto_increment_mask` 属于芯片寄存器命令协议，应放在 driver/chip readout 描述中；v1 不加入 profile。
- `turnaround_bytes` 是具体 read transaction 的参数，复用当前 `Turnaround` 思路，不固化到整个 Sensor profile。
- `dummy_byte` 只有出现要求特定 dummy value 的芯片时再加到 transaction option；v1 不需要。

多个 Sensor 可以共享相同 profile。`SpiProfile`/`I2cProfile` 是很小的值类型，Board profile 可以定义一个 `const` 并复制进多个 `BusBinding`，不需要 profile ID、引用表或 registry。profile 属于 Sensor 的 bus binding：它描述“访问这颗 Sensor 时总线必须处于什么参数”，共享 bus owner 在每次 transaction 内根据 binding 应用配置；即使多个 Sensor 当前参数相同，也不能假设它们永远跟随 bus 全局默认值。

把 route 和 profile 合并成一个 enum 比两个字段更简单，也让 `SPI route + I²C profile` 这类非法状态无法构造。`BusBinding` 不是额外包装层，而是替代原来的 `BusTarget + BusProfile`。

## 5. Sensor 配置与能力

Sensor 配置使用强类型值对象：

```rust
pub struct RangeG(pub u16);
pub struct RangeDps(pub u16);
pub struct SampleRateHz(pub u16);

pub struct ImuSampleConfig {
    pub accel_range: RangeG,
    pub gyro_range: RangeDps,
    pub sample_rate_hz: SampleRateHz,
}

pub enum SampleConfigCapability {
    Independent {
        accel_ranges: Vec<RangeG>,
        gyro_ranges: Vec<RangeDps>,
        sample_rates: Vec<SampleRateHz>,
    },
    Constrained {
        configs: Vec<ImuSampleConfig>,
    },
}

pub struct SampleReadoutRequest {
    pub temperature: bool,
    // pub sensor_timestamp: bool, // [后续：首个 driver 真正读取芯片 timestamp 后启用]
}
```

`SensorTimestampCapability` 整体属于后续能力。它描述 IMU 芯片内部 timestamp counter，而不是 ESP 单调时钟：

```rust
pub struct SensorTimestampCapability { // [后续]
    pub tick_hz: u32, // counter 每秒递增多少 tick，用于换算时间
    pub counter_bits: u8, // counter 位宽，用于处理回绕
    pub resets_on_sensor_reset: bool, // reset 后计数是否清零，用于判断时间连续性
}
```

它是只读的静态 capability descriptor，回答“这个 driver 能否读取芯片内部 timestamp，以及如何解释读出的 counter”，不负责 enable、reset、设置频率或选择时钟源。现有芯片 profile 的其余字段完整保留，v1 目标定义为：

```rust
pub struct TemperatureScale {
    pub c_per_lsb: f32,
    pub offset_c: f32,
}

pub struct ImuChipProfile {
    pub model: ImuChipModel,
    pub sample_config_capability: SampleConfigCapability,
    pub temperature_scale: Option<TemperatureScale>,
    // pub sensor_timestamp: Option<SensorTimestampCapability>, // [后续]
}
```

`ImuChipProfile` 是 Driver 的静态芯片能力描述，当前目标链路中有四个明确读取点：

```text
DriverInfo.chip_profile
  -> verify_identity: 用 model 形成 VerifiedIdentity
  -> configure: 用 sample_config_capability 校验 ImuSampleConfig
  -> raw-to-physical: 用有效 config 计算 Imu6Scale，用 temperature_scale 转换温度
  -> GetSensorInfo: 向 Host 返回该 Sensor 的静态采样能力
```

它不保存 Board route、当前有效配置、校准结果或运行时状态。当前代码的 `sensor_timestamp: bool` 在所有 driver 中均为 `false`，因此不把这个无有效实现的 bool 带入 v1 目标结构。首个 driver 真正读取芯片 timestamp 后，再使用 `Option<SensorTimestampCapability>` 同时表达“支持”和 counter 解释规则。如果某个芯片的 timestamp 时钟可配置，应另外定义该 driver 的 timestamp config/capability，并把最终生效的 `tick_hz` 放进 configured runtime；不能用这个固定描述结构冒充设置 API。

未来读取链路是：

```text
DriverInfo / ImuChipProfile
  -> 声明可读取和 counter 解释规则
Driver read_sample / FIFO read
  -> 返回原始 SensorTimestamp ticks
Sensor runtime clock tracker
  -> 处理 counter wrap/reset
  -> 映射到 Device 单调时钟 TimestampUs
Fusion / Event / Host sample
  -> 正常使用统一 TimestampUs
```

raw `SensorTimestamp` 主要用于 driver 诊断、FIFO 展开和时间同步；Host 常规 sample 不需要直接依赖芯片私有 tick 域。当前 `hxy42688`、`icm42688`、`lsm6` 和 `qmi8658` 的 `ImuChipProfile.sensor_timestamp` 全部为 `false`，Driver 也没有读取芯片 timestamp。因此 v1 不在 request、sample 或 capability 中启用该字段；设备采样时间统一使用 ESP 单调时钟生成的 `TimestampUs`。不能仅根据芯片数据手册可能存在 timestamp register 就宣称项目已支持。

## 6. 坐标系与校准

```rust
pub enum SignedAxis {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

pub struct AxisMapping {
    pub board_x: SignedAxis,
    pub board_y: SignedAxis,
    pub board_z: SignedAxis,
}

pub enum CalibrationSource {
    Factory,
    Board,
    User,
}

pub struct ImuCalibration {
    pub accel_bias_g: [f32; 3],
    pub accel_correction: [[f32; 3]; 3],
    pub gyro_bias_dps: [f32; 3],
    pub gyro_correction: [[f32; 3]; 3],
    pub reference_temperature_c: Option<f32>,
    pub source: CalibrationSource,
    pub revision: u32,
}
```

`AxisMapping` 来自 Board profile 和 `docs/hardware.md`，不属于 calibration 参数。字段语义是“某个 Board 轴取 Sensor 的哪个带符号轴”；例如 `board_x: NegY` 表示 `board.x = -sensor.y`。恒等安装为 `{ board_x: PosX, board_y: PosY, board_z: PosZ }`。处理顺序是先在 Sensor frame 应用 bias/correction，再转换到 Board frame，最后进入 fusion。温度补偿系数和 calibration quality metadata 可以在需求确定后扩展。

## 7. Sample 类型

```rust
pub struct RawImu6 {
    pub accel: [i16; 3],
    pub gyro: [i16; 3],
}

pub struct RawTemperature {
    pub raw: i16,
}

pub struct SensorTimestamp { // [后续]
    pub ticks: u32,
}

pub struct RawImuSample {
    pub imu6: RawImu6,
    pub temperature: Option<RawTemperature>,
    // pub sensor_timestamp: Option<SensorTimestamp>, // [后续]
}

pub struct Imu6Scale {
    pub accel_g_per_lsb: f32,
    pub gyro_dps_per_lsb: f32,
}

pub struct ImuSampleScale {
    pub imu6: Imu6Scale,
    pub temperature: Option<TemperatureScale>,
}

pub struct PhysicalImu6 {
    pub accel_g: [f32; 3],
    pub gyro_dps: [f32; 3],
}

pub struct PhysicalTemperature {
    pub celsius: f32,
}

pub struct PhysicalImuSample {
    pub imu6: PhysicalImu6,
    pub temperature: Option<PhysicalTemperature>,
    // pub sensor_timestamp: Option<SensorTimestamp>, // [后续]
}

pub struct Quaternion {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
```

StartSampling 成功时先产生 Stream descriptor。v1 不允许在同一 Stream 内热修改采样配置或 Fusion 配置；任何此类变化都停止旧 Stream、reset 必要状态并分配新的 `StreamId`：

```rust
pub struct StreamDescriptor {
    pub stream_id: StreamId,
    pub sensor_id: SensorId,
    pub effective_config: ImuSampleConfig,
    pub output: StreamOutput,
}

pub enum StreamOutput {
    Raw,
    Physical,
    Orientation {
        fusion: FusionConfig,
    },
}
```

每条 sample 或 orientation 共享同一精简元数据；采样和 Fusion 配置由 `StreamId` 对应的 descriptor 解释。`timestamp_us` 的语义固定为 **sample acquisition time**：v1 在 sample/read pipeline 中读取 ESP 单调时钟并写入，不使用 Event 编码或发送时刻。Orientation 继承产生它的同一 sample 时间戳，Fusion 的 `dt` 由相邻 sample acquisition timestamp 计算：

```rust
pub struct SampleMetadata {
    pub sensor_id: SensorId,
    pub stream_id: StreamId,
    pub sample_index: SampleIndex,
    pub timestamp_us: TimestampUs,
}

pub struct RawSamplePayload {
    pub metadata: SampleMetadata,
    pub sample: RawImuSample,
}

pub struct PhysicalSamplePayload {
    pub metadata: SampleMetadata,
    pub sample: PhysicalImuSample,
}

pub struct OrientationPayload {
    pub metadata: SampleMetadata,
    pub quaternion: Quaternion,
}
```

高频链路使用协议长度受限的 batch，Rust 容器仍使用 `Vec`：

```rust
pub struct SampleBatch {
    pub sensor_id: SensorId,
    pub stream_id: StreamId,
    pub first_sample_index: SampleIndex,
    pub base_timestamp_us: TimestampUs,
    pub records: Vec<SampleBatchRecord>,
}

pub struct SampleBatchRecord {
    pub delta_us: u32,
    pub sample: RawImuSample,
}
```

共享数据模型直接使用 `alloc::vec::Vec`。协议 decoder 在分配或填充前检查协议定义的最大 records 数和消息长度，不能因为使用 `Vec` 就接受无界输入。单条 `RawSamplePayload` 仍用于低速、调试和按次读取。

## 8. Driver 与身份验证

```rust
pub enum ProbeRegisterMatch {
    WhoAmI(u8),
    WhoAmIAndRevision {
        who_am_i: u8,
        revision: u8,
    },
    WhoAmIAndNotRevision {
        who_am_i: u8,
        revision: u8,
    },
}

pub struct ProbeRegisterReadout {
    pub who_am_i_register: u8,
    pub revision_register: Option<u8>,
    pub matches: &'static [ProbeRegisterMatch],
    pub attempts: u8,
    pub retry_delay_ms: u64,
}

pub enum SampleByteOrder {
    BigEndian,
    LittleEndian,
}

pub enum DataReadyCondition {
    AnySet,
    Equals(u8),
}

pub struct DataReadyStatus {
    pub register: u8,
    pub mask: u8,
    pub condition: DataReadyCondition,
}

pub struct SampleRegisterReadout {
    pub data_start_register: u8,
    pub byte_order: SampleByteOrder,
    pub status: Option<DataReadyStatus>,
    pub poll_attempts: u8,
    pub poll_delay_ms: u64,
    pub read_on_timeout: bool,
}

pub struct DriverInfo {
    pub name: &'static str,
    pub driver: &'static dyn ImuDriver,
    pub chip_profile: &'static ImuChipProfile,
    pub identity_check: ProbeRegisterReadout,
    pub sample_readout: SampleRegisterReadout,
}

pub struct VerifiedIdentity {
    pub model: ImuChipModel,
    pub identity: ImuIdentity,
}

pub enum IdentityCheckOutcome {
    Verified(VerifiedIdentity),
    Mismatch {
        expected: ImuChipModel,
        observed: ImuIdentity,
    },
    CommunicationFailure(SmartImuError),
}

#[async_trait(?Send)]
pub trait ImuDriver: Sync {
    fn info(&self) -> &'static DriverInfo;

    async fn verify_identity(
        &self,
        bus: &mut dyn ImuBus,
        binding: &BusBinding,
    ) -> IdentityCheckOutcome;

    async fn reset(
        &self,
        bus: &mut dyn ImuBus,
        binding: &BusBinding,
    ) -> Result<(), SmartImuError>;

    async fn configure(
        &self,
        bus: &mut dyn ImuBus,
        binding: &BusBinding,
        config: &ImuSampleConfig,
    ) -> Result<(), SmartImuError>;

    async fn read_sample(
        &self,
        bus: &mut dyn ImuBus,
        binding: &BusBinding,
        request: SampleReadoutRequest,
    ) -> Result<RawImuSample, SmartImuError>;
}
```

v1 的 topology 已知每个位置安装的 `ImuChipModel`，并配置唯一 `BusBinding`。启动时通过 `ImuChipModel` 从现有 `DriverInfo` registry 取得唯一 driver，由 bus owner 应用 binding 中的 profile，然后复用 `ProbeRegisterReadout` 读取 WHO_AM_I/revision 验证身份；不遍历 candidate driver，也不尝试猜测型号。开发板的用途是比较五颗已知 IMU 的性能，不等于硬件型号未知。

领域错误直接以现有 `SmartImuError` 为基线，并随命名迁移补充 identity mismatch：

```rust
pub enum UnsupportedConfigReason {
    SampleConfig,
    AccelRange,
    GyroRange,
    TemperatureReadout,
    // SensorTimestampReadout, // [后续]
}

pub enum SmartImuError {
    CommunicationError,
    ChipNotFound,
    SensorNotFound,
    IdentityMismatch,
    ConfigError,
    DataNotReady,
    MissingResource,
    UnsupportedConfig(UnsupportedConfigReason),
    InvalidTarget,
}

pub type SmartImuResult<T> = Result<T, SmartImuError>;
```

v1 不需要公共 `DriverId`、`ProbePlan`、`ProbeCandidate` 或 `Ambiguous` 结果。以后如果真有可热插拔的通用 Sensor 插槽，再单独增加 discovery 流程。多个寄存器兼容型号可以在 driver 模块内部复用实现，但 topology 的每个 `ImuChipModel` 仍解析到唯一 driver。

## 9. Sensor runtime

初始化 typestate：

```rust
pub struct ImuDevice<State> {
    pub driver: &'static dyn ImuDriver,
    pub bus: BusBinding,
    pub identity: ImuIdentity,
    pub state: State,
}

pub struct Verified;
pub struct Configured {
    pub config: ImuSampleConfig,
}
```

Device 动态生命周期：

```rust
pub enum SensorLifecycle {
    Disabled,
    Verifying,
    Verified,
    Configuring,
    Configured,
    Sampling,
    Faulted,
    Recovering,
}

pub enum SamplingStreamState {
    Stopped,
    Starting,
    Streaming,
    Stopping,
    Faulted,
}

pub struct SensorRuntime {
    pub sensor_id: SensorId,
    pub lifecycle: SensorLifecycle,
    pub stream_state: SamplingStreamState,
    pub effective_config: Option<ImuSampleConfig>,
    pub stream_id: Option<StreamId>,
    pub sample_index: SampleIndex,
    pub calibration: ImuCalibration,
    pub last_sample_timestamp_us: Option<TimestampUs>,
    pub last_error: Option<SmartImuError>,
    pub retry_count: u8,
}
```

Driver reference、Fusion algorithm instance 和 EventDetector 是 runtime 内部状态，不进入 wire descriptor。

## 10. Fusion algorithm

当前 `FusionFilter` 是平台无关的 6-axis AHRS 实现，输入加速度计、陀螺仪和外部 `dt`，支持 `NWU/ENU/NED` convention，不使用磁力计。v1 直接复用这套实现，不提前建立 trait object、registry 或“算法种类 + settings enum”两套可能不匹配的字段：

```rust
pub enum FusionConvention {
    Nwu,
    Enu,
    Ned,
}

pub struct FusionFilterSettings {
    pub convention: FusionConvention,
    pub gain: f32,
    pub gyroscope_range_dps: f32,
    pub acceleration_rejection: f32,
    pub recovery_trigger_period: u32,
}

pub struct FusionInput {
    pub sample: PhysicalImuSample,
    pub dt_s: f32,
}

pub struct FusionOutput {
    pub quaternion: Quaternion,
}

pub enum FusionConfig {
    Ahrs6Axis(FusionFilterSettings),
}

pub struct FusionFilter {
    // quaternion、initialisation/recovery 等算法状态均为 private
}

impl FusionFilter {
    pub fn new(settings: FusionFilterSettings) -> Self;
    pub fn reset(&mut self);
    pub fn update_imu(
        &mut self,
        accel_ms2: [f32; 3],
        gyro_rads: [f32; 3],
        dt_s: f32,
    ) -> Quaternion;
}
```

每个需要 Orientation 输出的 Sensor 独立持有一个 `FusionFilter` 实例。`FusionConfig` 用单一 enum variant 原子绑定算法与设置，避免 `algorithm = A` 却携带 B settings 的非法组合；`StreamDescriptor.output` 保存该配置，单条 `OrientationPayload` 只通过 `StreamId` 引用 descriptor。

第二种算法真正实现时，再增加 `FusionConfig` variant 和内部 static-dispatch `FusionEngine` enum，并抽取共同的 `reset/update` 行为；不需要现在先引入动态 registry 或 trait object。v1 的算法/设置变化会结束旧 Stream、reset 新实例并分配新 `StreamId`；只有后续支持 Stream 内热切换时才增加 `FusionRevision`。

现有 `FusionFilterSettings.magnetic_rejection` 在无磁力计执行路径中没有效果，目标设置结构删除该字段，也不宣称支持磁场融合或磁场拒绝。

## 11. Protocol 边界

Host/Device application message、稳定 wire code、UART/ESP-NOW framing、HostClient pending request 和协议兼容规则已拆到独立的 [protocol.md](protocol.md)。本文只保留协议会引用的领域类型：

- Device/Sensor/Request/Stream ID。
- `DeviceIdentity`、`DeviceTopology` 和 `SensorLifecycle`。
- `ImuSampleConfig`、`SampleReadoutRequest` 和 `StreamDescriptor`。
- Raw/physical/orientation sample payload。
- `SmartImuError`，由协议层映射为稳定错误码。

边界规则：

- `SmartImuActor` 接收解码后的 `Command` 并输出 `DeviceResponse`/`DeviceEvent`，但不处理 wire code、COBS、CRC 或 ESP-NOW datagram。
- `LinkId` 只用于 Device 内部回程路由，不进入 wire。
- 协议层可以引用领域类型，领域模型不能依赖具体 Link framing。
- Power telemetry 没有硬件 backend，不进入 v1 Command/Event。

## 12. SmartImuRuntime

```rust
pub struct SmartImuRuntime {
    pub identity: DeviceIdentity,
    pub boot_session_id: BootSessionId,
    pub topology: DeviceTopology,
    pub sensors: Vec<SensorRuntime>,
    pub next_stream_id: StreamId,
    // bus owners、每 Sensor FusionFilter 和 detector 为内部资源
}
```

核心行为：

```text
bootstrap topology
execute validated sensor operation
schedule bus operation
start/stop sampling stream
return domain result/event to Actor
recover faulted sensor
```

## 13. SmartImuActor

Actor 是 Device 业务状态的唯一可变入口。Link、timer 和其他任务只能投递消息，不能直接操作 `SmartImuRuntime`、`ImuManager` 或 Sensor：

```rust
pub enum ActorMessage {
    LinkOpened {
        link_id: LinkId,
    },
    LinkClosed {
        link_id: LinkId,
    },
    HostCommand {
        link_id: LinkId,
        command: Command,
    },
    SamplingTick {
        now: TimestampUs,
    },
    RecoveryTick {
        now: TimestampUs,
    },
}

pub enum ActorOutput {
    Reply {
        link_id: LinkId,
        response: DeviceResponse,
    },
    Publish {
        event: DeviceEvent,
    },
}

pub enum DeviceActorState {
    Booting,
    Verifying,
    Ready,
    Degraded,
    Faulted,
}

pub struct SmartImuActor {
    pub state: DeviceActorState,
    pub runtime: SmartImuRuntime,
}
```

Actor API 保持 executor-neutral：

```text
handle(message, now) -> zero or more bounded ActorOutput
```

`smartimu-esp` 可以用 Embassy task 和有界 channel 驱动 Actor；host tests 可以直接逐条调用 `handle`，不依赖真实 executor 或时间。Command Response 记录原始 `link_id`，保证响应返回请求来源；Publish Event 再由 Link Manager 按订阅分发。

Actor mailbox 和 outbox 必须有固定容量。控制消息、Response 和严重 Error 的优先级高于可丢弃的高频 sample telemetry。

## 14. 数据模型不变量

- `DeviceId` 在设备重启后保持稳定，`BootSessionId` 必须变化。
- `SensorId`、`BusId` 只在所属 Device 内解释。
- 一个 Response 必须且只能关联一个 `RequestId`。
- Sample 和 Orientation 使用同一 `SampleMetadata` 关联；`timestamp_us` 表示 sample acquisition time，当前由 ESP 单调时钟产生，不表示消息发送时间。
- v1 中采样配置或 Fusion 配置变化必须结束旧 Stream、reset 必要状态并产生新的 `StreamId`；不允许 Stream 内热修改。
- 后续若支持 Stream 内热配置，才启用 `ConfigRevision`/`FusionRevision`，且新 sample 不得引用旧 revision。
- 一个 Stream 的 `SampleIndex` 只在对应 Sensor 内解释。
- wire 中所有列表、字符串、packet 和 pending request 都有容量上限。
- 只有 `SmartImuActor` 可以修改 Device runtime 和 Sensor 生命周期。
- FusionConfig 或 settings 变化时必须 reset 对应 Sensor 的 Fusion 实例；v1 同时分配新 `StreamId`，后续热切换模式才递增 `FusionRevision`。
- Device 全局消息序号不能用于多 Link 丢包检测；排序、分片、重传序号由每条 Link/peer 独立管理。
- GPIO、HAL handle、driver trait object 和 Fusion state 不进入 wire 类型。
- 对外 wire tag 在协议 v1 冻结后只能按兼容规则扩展，不能随意重排。
