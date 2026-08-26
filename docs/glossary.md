# SmartIMU 术语表

本文定义 SmartIMU 文档、代码和协议中的标准术语。新增公共类型、协议角色或架构组件时，应先在这里确认命名，避免同一概念使用多套名称。

## 系统角色

| 标准术语 | 中文含义 | 定义 | 避免使用 |
|---|---|---|---|
| Host | 上位控制端 | 向 Device 发送 Command，并接收 Response/Event；可以是 PC、手机或另一块 MCU | 把 Host 固定理解为 PC |
| Device / SmartIMU Device | SmartIMU 设备端 | 运行 SmartIMU Device 固件、管理 Sensor 和通信链路的完整设备 | 用 Device 指代单颗 IMU |
| Sensor / IMU Sensor | IMU 传感器 | 通过 SPI/I²C 连接到 Device 的具体 IMU 芯片实例 | 用 Sensor 指代整块 ESP 设备 |
| ESP Controller | ESP 主控 Host | 使用 `smartimu-host` 控制 SmartIMU Device 的另一块 ESP 主控板 | 下位机、第二设备等模糊名称 |
| Viewer | 可视化 Host 应用 | PC 上观察、配置、记录和回放 SmartIMU 数据的应用 | 把 Viewer 当成协议实现 |

系统关系：

```text
Host
  <-> UART / ESP-NOW (v1；BLE/Wi-Fi 后续)
SmartIMU Device
  <-> SPI / I2C
IMU Sensor
```

## Device 架构组件

| 标准术语 | 定义 |
|---|---|
| `SmartImuActor` | Device 业务状态的唯一可变入口；接收 ActorMessage，控制 Runtime/Sensor，并产生 ActorOutput |
| Actor Inbox | 有界输入队列，接收 Host Command、sampling tick、recovery tick 和 link lifecycle message |
| Actor Outbox | 有界且有优先级的输出队列，承载 Response 和 Event；Link lifecycle/control 由 Link Manager 自身管理 |
| `SmartImuRuntime` | Device 的领域运行时，保存 topology、Sensor runtime、boot session 和 stream 状态 |
| `ImuManager` | 执行已知型号的 identity verification、configure/read/recovery，并协调 bus owner |
| Bus Owner | 一条物理 SPI/I²C bus 的唯一所有者，原子执行 profile、target select、transfer 和 release |
| `EventDetector` | 根据 physical sample/orientation 检测显著运动、姿态变化、静止、冲击等业务事件 |

Actor、Runtime 和 Manager 不直接承担 wire framing 或平台 HAL API。

## Hardware 与 Topology

| 标准术语 | 定义 |
|---|---|
| Board Profile | 某块实际硬件的编译期定义，包括 MCU、pin、bus、Sensor topology 和默认配置 |
| `DeviceTopology` | Device 内 buses 和 sensors 的静态关系；开发板为 5 Sensor，量产设备通常为 1 Sensor |
| Slot | 开发板上的物理 Sensor 安装位置，例如 slot 1；不是通用协议身份 |
| `BusId` | Device 内一条物理/逻辑 bus 的局部 ID |
| `SensorId` | Device 内一个 Sensor 的局部 ID |
| `BusBinding` | Sensor 的总线路由与访问参数绑定：SPI 保存 bus/CS/`SpiProfile`，I²C 保存 bus/address/`I2cProfile`；不是 Host Command target |
| `ImuChipModel` | 直接复用现有 enum；topology 预先声明的 IMU 芯片型号，v1 中每个 model 唯一解析到一个 driver |
| Bus Profile | `BusBinding` 中 Board 为该 Sensor 验证过的 SPI/I²C 参数；小值类型可由多个 Sensor 复制共享，v1 不遍历候选 profile |

`docs/hardware.md` 是 5 IMU 开发板 pin、slot、axis mapping 和电气关系的事实来源。

## Driver 与身份验证

| 标准术语 | 定义 |
|---|---|
| `drivers` module | 统一存放 driver contract、身份验证公共逻辑和 `drivers/<sensor>.rs` 芯片实现；不再同时保留顶层 `driver`/`drivers` 两套命名 |
| `ImuDriver` | 所有 IMU driver 实现的公共契约 |
| Driver | 一个芯片型号或兼容芯片族的寄存器协议实现，不包含 Board、Fusion、Actor 或 Host 协议逻辑 |
| Driver Registry | 根据 topology 的 `ImuChipModel` 返回唯一 driver；兼容型号可以在模块内部复用实现 |
| `DriverInfo` | Driver 的静态描述，包括 model、expected identity、capability 和 sample readout |
| Identity Verification | 使用 topology 的 `BusBinding` 访问 WHO_AM_I/revision，并与预期 `ImuChipModel` 对比；不是动态型号发现 |
| Identity Signature | WHO_AM_I/revision 等芯片识别信息；不是 Sensor 的全局唯一身份 |
| Dynamic Discovery | 后续可选能力；只有出现未知、可热插拔 Sensor 插槽时才设计，v1 不遍历候选 driver/profile |

## Protocol 与 Link

| 标准术语 | 定义 |
|---|---|
| Application Protocol | SmartIMU 统一的 Command、Response、Event 和数据类型语义 |
| Command | Host 发给 Device 的请求，携带 `RequestId`；只有 Sensor/Stream 操作的具体 body 携带 `SensorSelection`/`StreamSelection`，不使用所有命令共享的通用 target |
| Response | Device 对 Command 的结果，使用 `in_reply_to` 关联原 `RequestId`；不另设 `ResponseId`，Link/packet message ID 也不承担 application Response 身份 |
| Event | Device 主动或持续发布的消息，不属于某个 Command 的 Response |
| Link Protocol | 承载 application message 的链路规则；v1 只有 UART 和 ESP-NOW，BLE/Wi-Fi 后续按需加入 |
| `ProtocolLink` | application codec 与平台 I/O backend 之间的公共 Rust port |
| Link Backend | v1 为 PC/ESP 串口和 ESP-NOW 的具体 I/O 接入；手机 BLE/Wi-Fi backend 后续按需加入 |
| Link Manager | Device 上管理 UART session/ESP-NOW peer、分配 `LinkId`、路由 Response、维护 Event 订阅与 endpoint 生命周期的组件 |
| Wire Code | 与 Rust enum variant 顺序解耦的稳定数值编号；按 Message/Command/Event 类别解释，发布后不复用或重排 |
| Framing | 在字节流或 datagram 中划分消息边界，例如 UART COBS 或 TCP length prefix |
| Fragmentation | 后续能力；v1 消息必须适配单个 UART frame 或 ESP-NOW datagram，超限返回错误 |

不要为 PC 和 ESP Host 分别定义 SmartIMU application protocol；v1 共享 Rust 协议，只替换 UART/ESP-NOW backend。手机端及 BLE/Wi-Fi 后续按明确需求扩展。详细 wire code、payload 和兼容规则见 [`.plan/protocol.md`](../.plan/protocol.md)。

## Identity 与 Session

| 类型 | 阶段 | 定义 |
|---|---|---|
| `DeviceId` | v1 | SmartIMU Device 在 application protocol 中的稳定逻辑身份 |
| `McuHardwareId` | 后续 | MCU 硬件身份，例如 ESP eFuse base MAC；仅用于派生 DeviceId 或受控诊断 |
| `DeviceModel` | 后续 | 产品型号的稳定 tag，不表示某台设备实例 |
| `BootSessionId` | v1 | Device 的一次启动实例，重启后变化 |
| `SensorRef` | 后续 | Host SDK 的 `{ device_id, sensor_id }` 便利类型，不进入 Device-scoped wire payload |
| `LinkId` | v1/internal | Device 本次启动内单调分配的逻辑 Host endpoint/回程路由；关闭后不复用，不进入 wire protocol，也不是稳定 Host 身份 |
| `RequestId` | v1 | Host 生成，用于关联一次 Command/Response |
| `OperationId` | 后续 | 异步长任务需要进度、取消、查询或跨 Link 跟踪时，由 Device 生成 |
| `StreamId` | v1 | 一次 StartSampling 生命周期的 ID；v1 配置变化会产生新 Stream |
| `SampleIndex` | v1 | 一个 Sensor 在一个 Stream 内的样本序号 |
| `ConfigRevision` | 后续 | 只在支持同一 Stream 内热修改采样配置时启用 |
| `FusionRevision` | 后续 | 只在支持同一 Stream 内热切换 Fusion algorithm/settings 时启用 |
| `EventSeq` | 后续 | 非采样 Event 需要可靠缺失检测时启用，必须明确为订阅级或 Link/peer 级作用域 |
| `TimestampUs` | v1 | Device 单调时钟域中的微秒时间；sample metadata 中表示采样时刻，当前由 ESP 在 sample/read pipeline 中获取，不表示 Event 发送时刻 |
| `FirmwareVersion` | v1 | 固件的 SemVer 风格发布版本；用于展示、诊断和升级判断，不决定 wire 兼容性 |
| `ProtocolVersion` | v1 | application/wire protocol 兼容版本；独立于固件版本 |
| `DeviceCapabilities` | 后续 | 同一协议版本出现大量可选功能组合且 Host 需要统一发现时再引入；v1 不定义 |
| `LinkCapabilities` | 后续 | 出现 Link negotiation 需求时再引入；v1 UART/ESP-NOW 使用各自编译期限制 |

### `DeviceId` 与 MCU ID 的区别

`DeviceId` 标识完整 SmartIMU Device，而不是单颗 IMU，也不等同于裸 MCU ID。

开发阶段可以在 `smartimu-esp` 平台层由 `McuHardwareId`（例如 ESP eFuse base MAC）稳定派生 `DeviceId`。量产阶段也可以使用烧录的产品序列号，以便更换 MCU 后仍保留产品身份。`McuHardwareId` 属于后续受控诊断信息，不进入常规消息；具体身份策略由产品定义。

## Sample、坐标与 Calibration

| 标准术语 | 定义 |
|---|---|
| `RawImuSample` | Driver 从寄存器读取的原始整数样本 |
| `PhysicalImuSample` | 使用芯片 scale 转换后的物理单位样本 |
| `SensorTimestampCapability` | 后续只读静态能力：说明 Driver 能读取芯片内部 timestamp counter 及其 tick/回绕/reset 语义；不是 timestamp 设置 API |
| `SensorTimestamp` | 后续原始芯片 tick 值；由 Device runtime 映射到统一 `TimestampUs`，常规 Host sample 不直接依赖它 |
| Sensor Frame | IMU 芯片自身定义的 X/Y/Z 坐标系 |
| Board Frame | Board Profile 定义的设备统一 X/Y/Z 坐标系 |
| `AxisMapping` | Sensor Frame 到 Board Frame 的固定轴交换/取反关系；属于 topology/hardware |
| Calibration | bias、correction matrix、温度补偿等测量误差修正 |
| Factory Calibration | 出厂写入的产品校准参数 |
| Board Calibration | 与 PCB 安装和板级误差相关的校准参数 |
| User Calibration | 用户或运行时流程生成的校准参数 |
| Calibration Procedure | 静止零偏、六面体等生成 calibration 参数的状态机 |

推荐处理顺序：

```text
Raw register data
  -> chip scale in Sensor Frame
  -> Calibration in Sensor Frame
  -> AxisMapping to Board Frame
  -> Fusion
```

`AxisMapping` 不是 calibration 参数；flash/NVS 是 calibration persistence backend，不属于 calibration 算法本身。

## Fusion 与智能事件

| 标准术语 | 定义 |
|---|---|
| Fusion | 将校准后的 physical sample 融合为 orientation 的过程 |
| `FusionConfig` | 将 Fusion 实现与其 settings 原子绑定的配置 enum；v1 只有 `Ahrs6Axis(FusionFilterSettings)` |
| `FusionFilter` | 当前已有且 v1 唯一实现的无磁力计 6-axis AHRS |
| Fusion Engine | v1 为每个 Sensor 管理现有 `FusionFilter` 实例；第二种算法出现后通过 `FusionConfig` 与 static dispatch 扩展 |
| Orientation | Fusion 输出的姿态结果，当前主要表示为 Quaternion |
| Smart Event | `EventDetector` 产生的显著运动、姿态变化、静止、冲击等事件 |

当前只有 `FusionFilter` 是已实现算法。第二种算法真正出现后再增加 `FusionConfig` variant、内部 static-dispatch engine 和统一 contract test；Madgwick、Mahony 等名称不能在实现前写入支持列表。

## 状态术语

### Device Actor

```text
Booting -> Verifying -> Ready / Degraded -> Faulted
```

### Sensor Lifecycle

```text
Disabled -> Verifying -> Verified -> Configuring -> Configured -> Sampling
                                              -> Faulted -> Recovering
```

### Sampling Stream

```text
Stopped -> Starting -> Streaming -> Stopping -> Stopped
                    -> Faulted
```

完整转换、触发条件和交互时序见 [`.plan/architecture.md`](../.plan/architecture.md)。
