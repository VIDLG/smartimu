# 实机与发布验收

本文是可重复执行的构建和硬件验收清单，不保存一次性诊断结论。长期测试分层与 fake IMU 设计见 [测试策略](testing.md)。

## 范围

主链路包括：

- `crates/smartimu`
- `apps/esp32c3-board`
- `tools/imu-viewer`
- `tools/imu-viewer-wgpu`
- JSON transport
- Binary transport

当前代码限制：slot 3 未绑定 CS 且候选驱动为空，因此默认预期最多 4 个 active IMU。若启用 slot 3，必须先更新 [硬件](hardware.md)、board 配置和本验收标准。

## 1. Host 构建

使用 Pixi 提供开发环境，并通过 Justfile 运行仓库任务：

```bash
pixi run just check-host
```

通过标准：

- [ ] 命令退出码为 0。
- [ ] `smartimu` 默认 feature 可在 host 编译。
- [ ] 两个 viewer 均可编译。

## 2. 固件构建

### JSON transport

```bash
pixi run just check-device
```

### Binary transport

```bash
pixi run just check-device-binary
```

通过标准：

- [ ] 两种 transport 均能为 `riscv32imc-unknown-none-elf` 编译。
- [ ] feature 组合没有同时选择互斥 transport。

## 3. 烧录前检查

首次使用时安装项目固定版本的 `espflash`：

```bash
pixi run just espflash-install
```

conda-forge 当前不提供 `espflash`；该配方使用 Pixi 环境中的 Cargo 安装兼容 Rust 1.88 的 `espflash 4.5.0`。

- [ ] ESP32-C3 供电稳定。
- [ ] SPI 与 CS 映射和 [硬件](hardware.md) 一致。
- [ ] 目标串口未被 viewer、串口监视器或其他进程占用。
- [ ] 自动检测到唯一设备端口；若连接了多个设备，则已设置正确的 `ESPFLASH_PORT` 或向命令提供实际端口。

可先检查串口能否打开：

```bash
pixi run just serial-open-check
```

默认自动检测端口。多设备场景可显式执行 `pixi run just serial-open-check COM15 115200`，其中 `COM15` 必须替换为实际端口。

## 4. JSON 实机链路

### 构建与运行

```bash
pixi run just build-device
pixi run just run-device
```

也可先构建后使用 `espflash`：

```bash
pixi run just flash-device
```

`espflash` 默认自动检测端口；连接多个设备时可传入端口，例如 `pixi run just flash-device COM15`。

### 设备侧验收

- [ ] 固件正常启动，无 panic 或重启循环。
- [ ] 每个已启用槽位都有明确的 probe 成功或失败结果。
- [ ] slot 1、2、4、5 的 CS 相互独立，未出现同时选中。
- [ ] 成功设备持续产生 raw sample。
- [ ] 姿态输出保持有限值，不出现 NaN。
- [ ] heartbeat 中 active IMU 与成功配置的设备一致。

### Viewer 验收

```bash
pixi run just viewer
```

- [ ] 可选择实际串口。
- [ ] `Auto` 和 `Json` 模式能解码设备消息。
- [ ] inventory / probe 状态与设备输出一致。
- [ ] accel / gyro 曲线持续更新。
- [ ] 3D 姿态方向合理。
- [ ] 错误列表可见且不会阻塞其他 IMU。
- [ ] 录制、JSONL/CSV 导出与回放可用。

## 5. Binary 实机链路

```bash
pixi run just build-device-binary
```

烧录对应 binary transport 构建产物后：

- [ ] viewer 的 `Binary` 模式可以稳定分帧。
- [ ] `Auto` 模式可以识别 binary 输入。
- [ ] 连续运行期间无持续 CRC/COBS 错误。
- [ ] inventory、sample、orientation 和 heartbeat 与 JSON 模式语义一致。
- [ ] 断开重连后 session/sequence 状态可恢复显示。

Binary 仅“构建通过”不等于实机链路通过；发布验收必须完成真实串口连续流验证。

## 6. 自动化 HIL

先烧录当前工作区构建的默认 JSON firmware，然后执行：

```bash
pixi run just hil
```

命令默认自动检测唯一串口并观察 10 秒。多设备场景可执行 `pixi run just hil COM5`；如需延长观察时间，可执行 `pixi run just hil COM5 20`。

通过标准：

- [ ] 当前协议可以解码，无 JSON 协议帧损坏或版本不匹配。
- [ ] slot 1、2、4、5 的 probe/inventory 型号与 `docs/hardware.md` 一致。
- [ ] heartbeat 的 active IMU 恰好为 slot 1、2、4、5。
- [ ] 每个 active IMU 的 sample index 和时间戳持续递增。
- [ ] 每个 active IMU 至少产生 5 个 raw sample 和 orientation。
- [ ] raw accel/gyro 不会在整个观察窗口内保持逐位不变。
- [ ] orientation 四元数为有限值且近似归一化。
- [ ] slot 3 的 `ChipNotFound` 允许存在；active IMU 的瞬态 `DataNotReady` 会被计数但在持续产出有效样本时不视为失败；其他设备错误均失败。

该测试文件位于 `tests/hil/tests/esp32c3_board.rs`，带 `#[ignore]`，不会在普通 `test-host` 中访问硬件。

## 7. WGPU viewer

```bash
pixi run just viewer-wgpu
```

- [ ] 可连接并消费与稳定 viewer 相同的协议。
- [ ] 多个 active IMU 能同时显示。
- [ ] 长时间旋转更新无明显错乱或资源持续增长。
- [ ] 窗口缩放、切换和重连不会导致崩溃。

## 8. 完成标准

一次完整验收应记录：

- 日期、提交或版本标识。
- PCB 版本与实际芯片。
- 串口、transport、构建 profile。
- 每个 slot 的 probe 结果和 active 状态。
- host/device 构建命令结果。
- 两个 viewer 的验证范围。
- 已知例外及其是否阻塞发布。

只有实际执行过的项目才能标记为通过；历史报告见 [归档](archive/README.md)，不能代替当前验收记录。
