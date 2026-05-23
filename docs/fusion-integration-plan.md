# Fusion 集成计划

## 目标

为当前项目引入 `Fusion` 姿态解算库，将原始六轴数据转换为四元数，并让 GUI 支持两种查看模式：

- `Raw 6-Axis`
- `Quaternion`

在不同模式下，2D 和 3D 视图显示对应的数据。

## 参考来源

- 参考项目：
  - `C:\Users\yukikaza\Desktop\IMU\BMI088_Arduino\smartimu\algorithm_compare_c6`
- Fusion 库源码：
  - `C:\Users\yukikaza\Desktop\IMU\BMI088_Arduino\smartimu\src\Fusion`

## 总体方案

### 1. 新增 `imu-fusion` crate

新增：

- `crates/imu-fusion`

职责：

- 对 `Fusion` C 库做 Rust 封装
- 提供稳定的 Rust API
- 不把 C 头文件和底层细节泄漏到其他 crate

### 2. 第三方源码归档

将 Fusion 源码拷贝到当前仓库，例如：

```text
contrib/
└─ fusion/
   ├─ Fusion.h
   ├─ FusionAhrs.c
   ├─ FusionAhrs.h
   ├─ FusionAxes.h
   ├─ FusionCalibration.h
   ├─ FusionCompass.c
   ├─ FusionCompass.h
   ├─ FusionConvention.h
   ├─ FusionMath.h
   ├─ FusionOffset.c
   └─ FusionOffset.h
```

原则：

- 不直接引用桌面路径
- 让仓库构建可复现
- 版本固定，后续升级时显式变更

### 3. `imu-core` 新增姿态类型

新增核心类型：

- `Quaternion { w, x, y, z }`
- `OrientationSample`

协议扩展建议：

- 新增独立 `OrientationFrame`

字段建议：

- `header`
- `imu_id`
- `imu_kind`
- `sample_index`
- `timestamp_us`
- `quaternion`

不建议把 quaternion 塞进现有 `SampleFrame`：

- raw 和姿态语义不同
- 独立 frame 更清晰
- 后续回放和过滤更方便

## Fusion 封装设计

### Rust 封装 API

`imu-fusion` 暴露最小接口：

- `FusionFilter`
- `FusionSettings`
- `FusionFilter::new(settings)`
- `FusionFilter::reset()`
- `FusionFilter::update_imu(accel_ms2, gyro_rads, dt_s) -> Quaternion`

### 默认参数

先对齐参考项目参数：

- `convention = FusionConventionNwu`
- `gain = 6.0`
- `gyroscopeRange = 2000.0`
- `accelerationRejection = 10.0`
- `magneticRejection = 10.0`
- `recoveryTriggerPeriod = 0`

### 单位转换

Fusion 输入单位固定为：

- accelerometer: `m/s²`
- gyroscope: `rad/s`

所以设备侧更新前必须：

- raw accel -> `g`
- `g -> m/s²`
- raw gyro -> `dps`
- `dps -> rad/s`

### 姿态更新策略

采用：

- `FusionAhrsUpdateNoMagnetometer`

当前项目先不接磁力计。

关于“手动积分 yaw 后重建四元数”的策略：

- 默认第一阶段可以先直接输出 Fusion 原始 quaternion
- 但根据当前测试结论，后续**可能沿用参考项目中的“手动积分 yaw 后重建四元数”**
- 因此实现时不能把姿态输出路径写死为唯一策略

建议从一开始就支持两种姿态策略：

- `FusionNativeQuaternion`
- `FusionYawIntegratedQuaternion`

这样如果后续验证发现：

- Fusion 原生 yaw 漂移较大
- 或参考项目中的 yaw 积分重建策略更稳定

则可以直接切换，而不需要重构协议和 GUI。

## 设备侧集成

### 放置位置

设备侧集成放在：

- `apps/esp32c3-board`

具体做法：

- 每个已识别 IMU 挂一个独立的 `FusionFilter`
- 每次 sample 后：
  - raw -> physical
  - physical -> Fusion 单位
  - `update_imu(...)`
  - 根据当前姿态策略得到 quaternion
  - 发 `OrientationFrame`

若启用 `FusionYawIntegratedQuaternion`，则设备侧还需要：

- 保存积分 yaw 状态
- 从 Fusion 输出中提取 roll / pitch
- 用积分 yaw + Fusion roll/pitch 重建 quaternion

### 生命周期

每个 IMU 独立维护：

- filter state
- last update timestamp

重置场景：

- driver re-init
- IMU 重新探测成功
- 长时间 sample 中断后重新开始

### 范围控制

先只对当前已稳定工作的 4 路 IMU 启用：

- slot1
- slot2
- slot4
- slot5

`slot3/BMI270` 暂不阻塞 Fusion 主线。

## Viewer 改造

### 模式切换

在 `imu-viewer` 顶部增加模式切换：

- `Raw 6-Axis`
- `Quaternion`

### 2D 视图

#### Raw 6-Axis 模式

继续显示：

- accel `[g]`
- gyro `[dps]`

#### Quaternion 模式

改为显示：

- `qw`
- `qx`
- `qy`
- `qz`

每个 IMU 显示 4 条 quaternion 曲线。

### 3D 视图

#### Raw 6-Axis 模式

- 保留当前基于 accel/gyro 的简易预览
- 或在 UI 上标明其为近似预览

#### Quaternion 模式

- 用 quaternion 驱动真实姿态模型
- 3D 线框/坐标系直接按四元数旋转

### 回放与导出

录制、导出、回放需要同时支持：

- `SampleFrame`
- `OrientationFrame`

CSV 导出增加 quaternion 列：

- `qw`
- `qx`
- `qy`
- `qz`

## 实施顺序

1. 引入 `Fusion` 源码到 `contrib/fusion`
2. 新建 `crates/imu-fusion`
3. 为 `imu-core` 增加 quaternion / orientation 类型与协议
4. 在 `apps/esp32c3-board` 中为每个已识别 IMU 挂载 Fusion filter
5. 发出 `OrientationFrame`
6. 在 `imu-viewer` 中增加模式切换
7. 完成 quaternion 2D 曲线
8. 完成 quaternion 驱动的 3D 视图
9. 更新录制 / 导出 / 回放

## 验收标准

### 构建

- `imu-fusion` 可编译
- `esp32c3-board` 可编译
- `imu-viewer` 可编译

### 设备侧

- 已工作的 IMU 能稳定产出 quaternion
- `OrientationFrame` 能稳定输出

### Viewer

- `Raw 6-Axis` 模式正常
- `Quaternion` 模式正常
- 2D quaternion 曲线可见
- 3D 姿态可随运动变化

### 回归

- 录制/导出/回放包含 quaternion 数据
- 不影响现有 JSON/Binary transport 主链路

## 当前默认决策

- Fusion 在设备侧运行，不在 viewer 侧运行
- quaternion 通过独立 `OrientationFrame` 输出
- 当前先不处理 `slot3/BMI270`
- 默认先实现 `FusionNativeQuaternion`
- 同时明确保留 `FusionYawIntegratedQuaternion` 的切换能力
