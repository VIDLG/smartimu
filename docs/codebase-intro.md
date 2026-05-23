# 当前项目代码导览

这份文档是写给“Rust 还不太熟，但想尽快看懂这个仓库”的读者。

如果只用一句话概括这个项目，它在做的事情是：

1. 在 `ESP32-C3` 板子上通过 `SPI` 探测并读取多个 IMU 传感器。
2. 把采样数据编码成统一协议，通过串口发到电脑。
3. 在电脑端用 GUI 或 GPU viewer 把数据画出来，并显示姿态信息。

## 先不用怕 Rust

读这个项目时，你先把它当成“分层很清楚的 C/C++ 工程”也完全没问题。Rust 在这里主要是把接口、数据结构和错误处理写得更严格。

你会频繁看到下面这些 Rust 概念：

- `struct`：像 C 里的结构体，用来装数据。
- `enum`：枚举，但 Rust 的枚举比 C 强，既能表示状态，也能带数据。
- `trait`：可以理解成“接口”。
- `impl`：给类型实现方法，或者给某个类型实现某个接口。
- `Result<T, E>`：表示“成功返回 `T`，失败返回 `E`”。
- `Option<T>`：表示“有值或没值”。
- `static` / `const`：全局常量或静态对象。
- `no_std`：不依赖标准库，常见于嵌入式代码。
- `feature`：编译开关，用来切换 JSON / Binary 协议等行为。

你完全可以先只看“字段名、函数名、调用关系”，先不深究泛型、生命周期这些 Rust 细节。

## 仓库在结构上是什么

这是一个 **Rust workspace**。你可以把 workspace 理解成“一个总项目，里面拆了多个小库和小程序”。

根目录的 [`Cargo.toml`](../Cargo.toml) 里声明了这些成员：

```text
crates/
  imu-core
  imu-drivers
  imu-fusion
  imu-firmware
  imu-platform-esp
apps/
  esp32c3-board
tools/
  serial-open-check
  imu-viewer
  imu-viewer-wgpu
```

可以把它们理解成下面这几层：

- `imu-core`
  - 放“全项目都共享”的基础定义。
  - 比如总线接口、驱动接口、协议帧、IMU 类型、原始采样结构。
- `imu-drivers`
  - 放“某一种 IMU 芯片怎么读写寄存器”的具体实现。
- `imu-firmware`
  - 放“探测、初始化、协议打包”这类和业务流程有关、但不直接依赖某个硬件平台的逻辑。
- `imu-platform-esp`
  - 把上面的抽象接口真正接到 `esp-hal` 和 ESP32-C3 硬件上。
- `apps/esp32c3-board`
  - 最终跑在板子上的主程序。
- `tools/imu-viewer`
  - 基于 `egui/eframe` 的桌面查看器。
- `tools/imu-viewer-wgpu`
  - 基于 `wgpu` 的高性能 3D 查看器实验版本。

## 这个项目最重要的主线

如果你只想先抓住“代码是怎么跑起来的”，建议按下面这条主线理解。

### 1. 板级配置先定义“有几个槽位、每个槽位可能是什么传感器”

文件：[`apps/esp32c3-board/src/board.rs`](../apps/esp32c3-board/src/board.rs)

这里定义了：

- SPI 频率、采样周期、上电等待时间。
- 总线 profile，比如 `Mode0/Mode3`、`100kHz/500kHz/1MHz`。
- 5 个 IMU 槽位的静态配置 `BOARD_IMUS`。
- 每个槽位允许尝试哪些驱动 `candidates`。

这一步很像“硬件表”或“板级设备树”的简化版。

其中最重要的结构体是：

```rust
pub struct BoardImuConfig {
    pub imu_id: ImuId,
    pub target: ImuTargetId,
    pub label: &'static str,
    pub expected: ImuKind,
    pub candidates: &'static [CandidateDriver],
}
```

意思是：某个槽位对应哪个逻辑 IMU 编号、挂在哪条总线上、期望是什么型号、允许用哪些驱动去试探。

### 2. 主程序启动后，创建 SPI 和片选引脚

文件：[`apps/esp32c3-board/src/main.rs`](../apps/esp32c3-board/src/main.rs)

这里做了几件非常关键的事：

- 初始化 `esp-hal` 和 RTOS/定时器。
- 配置 `SPI2` 的 `SCK/MOSI/MISO`。
- 创建 5 个 `CS` 引脚。
- 用这些对象组装出 `EspImuBus`。

`EspImuBus` 定义在 [`crates/imu-platform-esp/src/bus.rs`](../crates/imu-platform-esp/src/bus.rs)，它是“硬件版总线实现”。

它做的事很直接：

- 切换 SPI 模式和频率。
- 拉低某个目标 IMU 的 CS。
- 执行 `write` / `transfer_in_place`。
- 再把 CS 拉高。

也就是说，`imu-core` 里定义的是“应该怎么访问总线”，而 `imu-platform-esp` 负责“在 ESP32-C3 上真的去访问总线”。

### 3. 固件会为每个槽位尝试探测驱动

主程序没有把“某个槽位一定是某个芯片”写死，而是调用：

- [`crates/imu-firmware/src/runtime.rs`](../crates/imu-firmware/src/runtime.rs) 里的 `probe_first_matching`

它的逻辑是：

1. 遍历这个槽位允许的候选驱动。
2. 对每个驱动，再遍历它允许的总线 profile。
3. 切 profile。
4. 调驱动的 `probe()`。
5. 一旦成功，就返回“驱动 + profile”的组合。

这个设计很重要，因为项目里的几种 IMU 在 SPI 模式、读寄存器 dummy byte、寄存器布局上并不完全一致。

### 4. 具体芯片差异，都藏在驱动里

文件目录：[`crates/imu-drivers/src`](../crates/imu-drivers/src)

这里每个文件基本对应一种芯片：

- `icm42688.rs`
- `hxy42688.rs`
- `bmi270.rs`
- `qmi8658.rs`
- `lsm6.rs`

这些驱动都实现了 [`crates/imu-core/src/driver.rs`](../crates/imu-core/src/driver.rs) 里的 `ImuDriver` trait：

```rust
pub trait ImuDriver: Sync {
    fn kind(&self) -> ImuKind;
    fn probe(&self, bus: &mut dyn ImuBus, target: ImuTargetId) -> Result<bool, ImuError>;
    fn reset(&self, bus: &mut dyn ImuBus, target: ImuTargetId) -> Result<(), ImuError>;
    fn configure(...);
    fn read_raw(...);
    fn scale_profile(&self) -> ScaleProfile;
    fn capabilities(&self) -> ImuCapabilities;
}
```

把它翻译成人话就是：

- `probe`：这颗芯片是不是我。
- `reset`：复位它。
- `configure`：把采样模式配起来。
- `read_raw`：读一帧原始数据。
- `scale_profile`：告诉上层原始值怎么换算成物理单位。
- `capabilities`：告诉上层这个芯片支持什么。

比如 [`crates/imu-drivers/src/icm42688.rs`](../crates/imu-drivers/src/icm42688.rs) 会：

- 读 `WHO_AM_I` 和修订号确认芯片身份。
- 写几个控制寄存器做初始化。
- 轮询状态寄存器，确认数据 ready。
- 再把加速度和陀螺仪原始值读出来。

而 [`crates/imu-drivers/src/bmi270.rs`](../crates/imu-drivers/src/bmi270.rs) 更特殊，因为 BMI270 需要上传一段配置 blob，所以它会在 `configure()` 里向芯片灌入配置数据。

## `imu-core` 是全项目最值得先看的 crate

如果你是第一次进这个仓库，最建议优先看的是 `imu-core`。

### 1. 总线接口

文件：[`crates/imu-core/src/bus.rs`](../crates/imu-core/src/bus.rs)

这里定义了：

- `BusId`
- `ImuTargetId`
- `BusMode`
- `BusProfile`
- `ImuBus`

最关键的是 `ImuBus` trait。它是整个项目的“硬件访问抽象层”。

驱动并不知道自己运行在 ESP32、Linux 还是别的平台，它只知道自己拿到了一个 `ImuBus`，可以读写寄存器。

### 2. 驱动接口

文件：[`crates/imu-core/src/driver.rs`](../crates/imu-core/src/driver.rs)

这里定义了所有驱动必须遵守的统一接口。你可以把它当成“驱动标准模板”。

### 3. 数据类型

文件：[`crates/imu-core/src/types.rs`](../crates/imu-core/src/types.rs)

这里定义了项目里的核心名词：

- `ImuKind`
- `ImuId`
- `ImuConfig`
- `ImuDescriptor`
- `ImuCapabilities`
- `ImuError`
- `Quaternion`

以后你在板端、协议层、viewer 里看到这些名字，基本都从这里来。

### 4. 采样结构

文件：[`crates/imu-core/src/sample.rs`](../crates/imu-core/src/sample.rs)

这里很重要，因为它把“原始寄存器值”和“物理量”区分开了：

- `RawSample`
- `PhysicalSample`
- `ScaleProfile`

最常见的调用是：

```rust
let physical = raw.to_physical(scale);
```

也就是先读出原始整数，再根据量程换算成 `g`、`dps` 这些更容易理解的单位。

### 5. 串口协议

文件：[`crates/imu-core/src/protocol.rs`](../crates/imu-core/src/protocol.rs)

这个文件定义了板子和 PC 之间传输的数据格式。

统一的顶层枚举是：

```rust
pub enum WireFrame {
    Hello(...),
    Topology(...),
    ProbeResult(...),
    Sample(...),
    Orientation(...),
    Error(...),
    Heartbeat(...),
}
```

可以把它理解成“串口上跑的消息协议”。

项目当前支持两种传输格式：

- `Json`
  - 方便调试，肉眼可读。
- `Binary`
  - 用 `postcard + COBS + CRC32`，更紧凑，也更适合稳定传输。

## 板端真正的业务流程是什么

回到 [`apps/esp32c3-board/src/main.rs`](../apps/esp32c3-board/src/main.rs)，主流程可以概括成：

1. 初始化硬件。
2. 等待上电稳定。
3. 发送一帧 `Hello`。
4. 对每个槽位做探测。
5. 探测成功后 `reset + configure`。
6. 发送 `ProbeResult`。
7. 发送整机 `Topology`。
8. 进入循环：
   - 切到对应 profile。
   - 读原始 IMU 数据。
   - 发 `Sample`。
   - 如果姿态融合已启用，再发 `Orientation`。
   - 定期发 `Heartbeat`。

这就是整个固件的“主循环”。

## 姿态融合是怎么接进来的

文件：[`crates/imu-fusion/src/lib.rs`](../crates/imu-fusion/src/lib.rs)

这个 crate 不是纯 Rust 算法实现，而是对 `contrib/fusion/` 下 C 库的一个 Rust 封装。

你会看到：

- `unsafe extern "C"`
- `FusionAhrs...` 这些函数声明

这表示 Rust 在调用外部 C 代码。

对应的 [`crates/imu-fusion/build.rs`](../crates/imu-fusion/build.rs) 会在编译时把 `contrib/fusion` 里的 C 文件一起编进来。

主程序中这段逻辑的作用是：

1. 把原始加速度和角速度换成物理单位。
2. 调 `FusionFilter::update_imu()`。
3. 拿到四元数 `Quaternion`。
4. 发送 `Orientation` 协议帧给上位机。

## BMI270 为什么看起来特别“重”

这是项目里另一个容易让新手疑惑的点。

BMI270 配置时需要一段配置 blob。这个 blob 的来源不是手写在 Rust 代码里，而是：

1. 原始数据放在 `contrib/bmi270/bmi270_upstream.c`
2. [`crates/imu-platform-esp/build.rs`](../crates/imu-platform-esp/build.rs) 在编译时把这个 C 文件里的数组提取出来
3. 生成 `bmi270_config.rs`
4. [`crates/imu-platform-esp/src/resources.rs`](../crates/imu-platform-esp/src/resources.rs) 再把它作为 `DriverResources` 提供给 BMI270 驱动

所以 BMI270 驱动会比其他驱动多一个“加载资源”的概念，这不是代码写乱了，而是芯片本身就需要这一步。

## PC 端 viewer 在做什么

### `imu-viewer`

文件：[`tools/imu-viewer/src/main.rs`](../tools/imu-viewer/src/main.rs)

这是一个桌面 GUI 程序，主要职责有：

- 枚举串口。
- 连接串口。
- 自动识别输入是 JSON 还是 Binary。
- 把字节流解码成 `WireFrame`。
- 维护当前 topology、最新 sample、历史曲线、错误列表。
- 画 2D 曲线和简单 3D 线框预览。
- 支持录制和回放。

它本质上就是协议消费者。

也就是说，板子发什么，它就按 `WireFrame` 一帧一帧收下来，然后更新界面状态。

### `imu-viewer-wgpu`

文件：[`tools/imu-viewer-wgpu/src/main.rs`](../tools/imu-viewer-wgpu/src/main.rs)

这个工具和 `imu-viewer` 的核心差别不在协议，而在渲染方式：

- 它还是收同样的 `WireFrame`
- 但绘图改用 `wgpu`
- 更偏向高刷新率、3D 姿态预览验证

你可以把它理解成“共享同一协议层，但换了一套渲染前端”。

## `imu-firmware` 和 `imu-platform-esp` 怎么分工

这两个 crate 很容易看混。

### `imu-firmware`

它更偏“平台无关的流程代码”，比如：

- `runtime.rs`
  - 怎么按候选驱动顺序做探测。
- `transport.rs`
  - 怎么生成 `Hello / Sample / Heartbeat` 这些协议帧。
- `resources.rs`
  - 提供一个空资源实现。

### `imu-platform-esp`

它更偏“ESP32 上的落地实现”，比如：

- `bus.rs`
  - 真的去操作 SPI 和 GPIO。
- `resources.rs`
  - 提供 BMI270 配置 blob。

一句话总结：

- `imu-firmware` 关心“流程”
- `imu-platform-esp` 关心“怎么在 ESP 上做出来”

## 目录里其他内容怎么理解

- `contrib/`
  - 第三方或上游材料。
  - 现在主要是 `fusion` C 库和 BMI270 上游配置文件。
- `example/`
  - 芯片资料 PDF。
- `docs/`
  - 架构、硬件、测试、排障、viewer 计划等文档。
- `apps/esp32c3-board/src/bin/`
  - 一些辅助 probe 程序，偏调试用途。
- `tools/serial-open-check`
  - 很小的串口打开检查工具，主要帮助定位 Windows 串口问题。

## 推荐阅读顺序

如果你现在就准备开始顺着源码看，建议用这个顺序：

1. [`crates/imu-core/src/types.rs`](../crates/imu-core/src/types.rs)
   - 先认名词。
2. [`crates/imu-core/src/bus.rs`](../crates/imu-core/src/bus.rs)
   - 理解总线抽象。
3. [`crates/imu-core/src/driver.rs`](../crates/imu-core/src/driver.rs)
   - 理解驱动接口。
4. [`crates/imu-core/src/protocol.rs`](../crates/imu-core/src/protocol.rs)
   - 理解板端和 PC 端怎么通信。
5. [`apps/esp32c3-board/src/board.rs`](../apps/esp32c3-board/src/board.rs)
   - 看板级配置。
6. [`apps/esp32c3-board/src/main.rs`](../apps/esp32c3-board/src/main.rs)
   - 看主流程怎么串起来。
7. [`crates/imu-platform-esp/src/bus.rs`](../crates/imu-platform-esp/src/bus.rs)
   - 看抽象如何落地到 SPI/GPIO。
8. 任选一个驱动，比如 [`crates/imu-drivers/src/icm42688.rs`](../crates/imu-drivers/src/icm42688.rs)
   - 看具体芯片怎么 probe / configure / read。
9. [`tools/imu-viewer/src/main.rs`](../tools/imu-viewer/src/main.rs)
   - 看上位机如何消费协议。

这个顺序的好处是：你会先建立“整体地图”，再进入细节，不容易迷路。

## 看这个仓库时，最值得记住的 5 句话

1. `imu-core` 定义规则，不直接碰硬件。
2. `imu-drivers` 只关心芯片寄存器，不关心底层 SPI 是谁实现的。
3. `imu-platform-esp` 负责把抽象接口接到 ESP32-C3 上。
4. `esp32c3-board` 是真正运行的固件入口。
5. 两个 viewer 只是协议消费者，核心协议都在 `imu-core`。

## 如果你接下来想继续深入

下一步最适合看的通常是两条路线之一：

- 想看“板子怎么采样”
  - 继续顺着 `apps/esp32c3-board/src/main.rs`
  - 读 `probe_first_matching`
  - 再读一个具体驱动
- 想看“上位机怎么显示”
  - 先读 `WireFrame`
  - 再读 `tools/imu-viewer/src/main.rs` 的 `handle_frame`

如果你愿意，我下一步还可以继续帮你补一份“按文件逐个解释”的版本，或者直接带你从 [`apps/esp32c3-board/src/main.rs`](../apps/esp32c3-board/src/main.rs) 开始逐段讲。 
