# `imu-viewer-wgpu` 计划

## Summary

保留现有 [imu-viewer](../tools/imu-viewer) 作为稳定版，不替换。新增一个并行实验性工具：

- `tools/imu-viewer-wgpu`

目标：

- 复用当前协议、串口、录制、回放、模式切换能力
- 用 `wgpu` 重做 3D 渲染路径，重点验证**多 IMU 同时 3D 展示**的性能是否优于现有 `egui/eframe`
- 尽量保持和现有 viewer 一致的使用方式，但优先把高性能多 IMU 3D 路径打通

## Key Changes

### 1. 新增独立工具，不替换现有 viewer

新增：

- `tools/imu-viewer-wgpu`

并保持：

- `tools/imu-viewer`
  - 继续作为当前可用稳定版
- `tools/imu-viewer-wgpu`
  - 作为高性能 3D 实验版

这样做的原因：

- 现有 viewer 已可用，不应在性能实验阶段被破坏
- `wgpu` 的渲染路径和 UI 组织方式与当前实现差异较大，适合并行推进

### 2. 复用协议与数据层

`imu-viewer-wgpu` 不重新定义协议，直接复用：

- `imu-core`
  - `WireFrame`
  - `SampleFrame`
  - `OrientationFrame`
  - `ImuDescriptor`
  - `Quaternion`
  - `default_scale_profile_for_kind`

串口输入模式保持一致：

- `Auto`
- `Json`
- `Binary`

Windows 下继续沿用当前已验证的串口策略：

- JSON 优先支持 PowerShell `System.IO.Ports.SerialPort` fallback
- Binary 继续使用 `serialport` crate
- 若后续需要，可把 PowerShell fallback 抽成共享模块

### 3. `wgpu` viewer 功能范围

第一阶段必须具备：

- 串口连接
- 输入模式切换
- 多 IMU 3D 视图
- 基本状态栏

第二阶段补齐：

- 2D 曲线
- IMU 列表与选中控制
- 错误列表
- 录制
- JSONL 导出
- CSV 导出
- 回放

原因：

- 本次立项的核心动机是多 IMU 3D 性能
- 所以多实例姿态渲染路径优先级高于其他 UI 完整性

### 4. 3D 视图设计

`imu-viewer-wgpu` 的目标是：

- 用 quaternion 驱动真正的 GPU 3D 姿态视图
- 在同一场景中同时渲染所有 IMU
- 将高频姿态更新从普通 UI 重绘中分离出来
- 尽量减少 CPU 端逐帧布局开销

建议策略：

- 2D 与状态面板低频更新
- 3D 使用独立渲染循环或高频 redraw
- 姿态更新支持插值/平滑
- 默认同时显示所有 IMU
- 若后续需要，再增加单 IMU 聚焦模式

### 5. 代码结构建议

`tools/imu-viewer-wgpu` 建议模块：

- `main`
  - 启动应用
- `serial`
  - 串口接入与 PowerShell fallback
- `state`
  - topology / sample / orientation / replay 状态
- `ui/sidebar`
  - IMU 列表与选中状态
- `ui/status`
  - 状态栏、错误、录制状态
- `ui/dashboard`
  - 2D 数据视图
- `render/scene`
  - 多 IMU 3D 场景组织
- `render/camera`
  - 相机与交互
- `render/mesh`
  - 线框或模型资源
- `replay`
  - 录制与回放

## Public Interfaces / Shared Types

保持复用，不新增协议分叉：

- `imu-core::WireFrame`
- `imu-core::SampleFrame`
- `imu-core::OrientationFrame`
- `imu-core::ImuDescriptor`
- `imu-core::Quaternion`

新增仅限 `imu-viewer-wgpu` 内部状态类型，例如：

- `ViewerState`
- `ViewMode`
- `ConnectionState`
- `RenderMode`
- `SelectedImuState`
- `GpuSceneState`

不修改设备侧协议字段。

## Test Plan

### 构建

- `cargo check -p imu-viewer-wgpu`
- 现有：
  - `cargo check -p imu-viewer`
  - `cargo check -p esp32c3-board --target riscv32imc-unknown-none-elf`
  继续保持通过

### 功能

- JSON 模式下可连接 `COM15`
- 能收到 `Hello / Topology / Sample / Orientation / Heartbeat`
- `Quaternion` 模式下多 IMU 3D 视图正常更新
- `Raw 6-Axis` 模式下 2D 视图正常更新
- 多 IMU 同时显示时不会卡死或错乱

### 性能

- 与现有 `imu-viewer` 对比主观帧率
- 重点观察：
  - 多 IMU 3D 旋转流畅度
  - 拖动/交互卡顿
  - CPU 占用变化
  - GPU 占用与温度是否合理
- 至少在 quaternion 模式下，多 IMU 同时 3D 展示体验应明显优于现有 viewer 才有保留价值

## Assumptions

- `imu-viewer` 保留，不做替换。
- `imu-viewer-wgpu` 是性能实验版，初期允许功能不完全对齐，但协议必须一致。
- 本次成功标准不是“把所有功能重新做一遍”，而是先证明 `wgpu` 的 3D 路径值得引入。
- Windows 仍然是当前主要开发环境，因此方案默认兼容 Windows。
