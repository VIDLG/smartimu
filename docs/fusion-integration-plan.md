# Fusion 集成说明

## 当前状态

姿态融合已经集成在 `crates/smartimu/src/fusion/mod.rs`，当前实现是纯 Rust，不依赖外部 C 源码、FFI 或融合算法专用 `build.rs`。

核心 API：

- `FusionFilter`
- `FusionFilterSettings`
- `FusionConvention`

固件侧读取原始六轴数据后，先按量程转换成物理单位，再调用 `FusionFilter::update_imu()` 更新姿态，最后通过协议发送四元数。

## 设计边界

- 当前融合路径使用加速度计和陀螺仪，不接磁力计。
- 算法实现留在 `smartimu` 内部，避免额外 crate 和跨语言构建链。
- 上位机只消费协议里的姿态结果，不需要知道融合算法的内部实现。
