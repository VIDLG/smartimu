# 故障排查

本页按“供电与连接 → SPI/CS → probe → 初始化 → 采样 → 串口/viewer”的顺序排查当前 workspace。旧单-crate时代的寄存器快照和一次性结论保存在 [历史归档](archive/README.md)。

## 先确认当前事实

1. 查看 [硬件](hardware.md) 的当前引脚表。
2. 核对 [`board.rs`](../apps/esp32c3-board/src/board.rs) 的候选驱动和 SPI profile。
3. 核对 [`main.rs`](../apps/esp32c3-board/src/main.rs) 的 `EspImuBus::with_target()` 绑定。
4. 注意 slot 3 当前禁用：没有候选驱动，也没有 CS target。

不要直接照搬归档文档中的 GPIO、旧 `src/...` 路径或 BMI270 结论。

## 快速决策表

| 症状 | 优先检查 |
|---|---|
| 所有槽位无响应 | 供电、SCK/MOSI/MISO、主控 SPI 初始化 |
| 单个槽位始终 `0xFF` | 该槽位 CS、焊接、芯片供电、MISO 连通性 |
| 固定但错误的 ID | SPI mode、频率、dummy/turnaround、寄存器地址 |
| probe 成功但 configure 失败 | reset/写入顺序、延迟、量程/ODR 支持 |
| 偶发成功、偶发失败 | 电源纹波、CS 默认电平、线长、频率、上电等待 |
| 有设备但没有 sample | data-ready、采样寄存器布局、轮询超时、配置结果 |
| 设备输出正常但 viewer 无数据 | 串口占用、transport 模式、波特率/USB 通道、分帧错误 |

## 1. 供电和物理连接

- [ ] 供电电压和地稳定。
- [ ] 芯片方向正确，无虚焊、桥接或缺件。
- [ ] SCK GPIO6、MOSI GPIO7、MISO GPIO2 与所有已启用 IMU 连通。
- [ ] 每个已启用 CS 与 [硬件](hardware.md) 当前表一致。
- [ ] 未访问时所有 CS 保持高电平。

如果结果随重启或触碰板子变化，先解决电气问题，不要用无限重试掩盖。

## 2. 解读常见 SPI 返回值

| 返回值模式 | 常见含义 | 下一步 |
|---|---|---|
| 全部 `0xFF` | 没有从设备驱动 MISO | 查 CS、供电、焊接和 MISO |
| 全部 `0x00` | 总线被拉低或设备异常 | 查短路、mode 和器件状态 |
| 稳定的非预期值 | 读错寄存器或时序不匹配 | 查地址、turnaround、mode、频率 |
| 正确 ID 后又丢失 | 上电/reset/供电或 CS 状态不稳定 | 延长等待并观察波形 |

降低 SPI 频率可用于定位信号完整性问题，但要记录原值、测试值和结果。当前基准为 1 MHz，配置位于 `board.rs`。

## 3. Probe 失败

检查顺序：

1. 目标是否实际注册到 `EspImuBus`。
2. `ImuTargetId.target_index` 是否与 CS 绑定一致。
3. 候选驱动是否包含实际芯片。
4. 候选 profile 是否包含芯片支持的 mode。
5. `WHO_AM_I`、revision 地址和值是否正确。
6. 芯片读取是否需要额外 turnaround/dummy byte。

若要增加诊断，优先记录“目标、profile、寄存器、返回值和错误”，不要只输出一个 `probe failed`。

## 4. Probe 成功但配置失败

- 核对驱动 `reset()` 和 `configure()` 的寄存器写入顺序。
- 核对 reset 后和关键写入后的最小延迟。
- 确认 `ImuSampleConfig` 在芯片能力范围内。
- 确认失败是否来自通信、unsupported config 或设备状态。
- 用 real driver + fake bus + chip fake 复现顺序问题，避免只靠实机反复试参数。

测试模式见 [测试策略：IMU driver tests](testing.md#imu-driver-tests)。

## 5. 有设备但没有采样

- 确认 configure 已成功，设备被加入 active 集合。
- 检查 data-ready 寄存器、mask 和条件。
- 检查 poll 次数、间隔和 `read_on_timeout` 行为。
- 检查数据起始寄存器、大小端和轴顺序。
- 不建议长期注释 data-ready 检查；先通过 fake model 证明是哪一层的问题。

## 6. 姿态异常

- 确认原始加速度和角速度先按正确量程转为物理单位。
- 确认 `dt` 为正且与采样时间一致。
- 检查坐标轴方向和四元数约定。
- 观察输出是否归一化、有限且没有 NaN。
- 用确定性 host 测试区分算法问题和硬件噪声。

见 [Fusion](fusion.md) 与 [测试策略：Fusion tests](testing.md#fusion-tests)。

## 7. 串口或 viewer 无数据

1. 关闭其他串口监视器和可能占用端口的进程。
2. 使用实际端口执行：

   ```bash
   pixi run just serial-open-check
   ```

3. 确认固件 transport 与 viewer 模式一致：JSON、Binary 或 Auto。
4. JSON 模式检查是否输出完整的一行一条消息。
5. Binary 模式检查 COBS `0x00` 分隔和 CRC 错误计数。
6. 重新连接后核对 session 和 sequence 是否更新。

命令默认自动检测端口。若存在多个候选端口，可显式执行 `pixi run just serial-open-check COM15 115200`；`COM15` 仅为示例，不是固定硬件配置。

## 提交问题时记录

- 当前提交或版本。
- PCB 版本、槽位和芯片表面标记。
- 实际引脚和供电测量。
- transport、SPI mode 与频率。
- 完整 probe/config/sample 错误，而不是截取单个值。
- 已执行的排查步骤及其结果。
