# IMU PCB 测试项目说明（历史资料）

## 项目概述

这是一份单 crate 时代的项目说明，保留为历史背景资料。当前仓库主线已经迁移到 workspace 结构，请优先参考：

- [README.md](../README.md)
- [architecture.md](architecture.md)
- [refactor-plan.md](refactor-plan.md)

本文档中的目录结构、命令和实现路径不再完全代表当前主实现。

## 支持的传感器

| 传感器型号 | 厂商 | WHO_AM_I 地址 | 期望值 |
|-----------|-------|---------------|---------|
| ICM-42688-HXY | 华轩阳电子 (HXY) | 0x01 | 0x6A |
| ICM-42688-PC | Tokmas (托克马斯) | 0x00 | 0x05 |
| BMI270 | Bosch (博世) | 0x00 | 0x24 |
| QMI8658A | QST (上海矽睿) | 0x00 | 0x05 |
| SC7U22 | SILAN (士兰微) | 0x01 | 0x6A |

## 项目结构

```
smartimu/  # 旧单 crate 时代结构示意
├── src/
│   ├── bin/
│   │   └── main.rs          # 主程序入口
│   ├── lib.rs              # 库模块导出
│   ├── spi.rs              # SPI 引脚配置和 IMU 类型定义
│   ├── imu.rs              # IMU 传感器错误定义
│   └── test.rs            # 测试结果类型和汇总函数
├── tests/
│   └── hello_test.rs      # 嵌入式测试示例
├── Cargo.toml            # 项目配置
└── readme.md            # PCB 硬件说明
```

## PCB 引脚定义

根据 [readme.md](readme.md)，引脚从左到右依次为：

- **GND** - 供电地
- **5V** - 供电5V（推荐）
- **3V3** - 内部3.3V LDO输出（也可输入3.3V）
- **INT** - 公共中断输出（所有传感器INT引脚并联）
- **MISO** - SPI 从→主 数据
- **MOSI** - SPI 主→从 数据
- **SCK** - SPI 时钟
- **ICM1** - ICM-42688-HXY CS 引脚（低电平选中）
- **ICM2** - ICM-42688-PC CS 引脚
- **BM** - BMI270 CS 引脚
- **SC** - SC7U22 CS 引脚
- **QMI** - QMI8658A CS 引脚

## ESP32-C3 引脚分配

当前代码中的引脚分配如下：

| 功能 | ESP32-C3 引脚 |
|-----|-------------|
| SPI MOSI | GPIO7 |
| SPI MISO | GPIO2 |
| SPI SCK | GPIO6 |
| ICM1 CS | GPIO1 |
| ICM2 CS | GPIO3 |
| BM CS | GPIO4 |
| QMI CS | GPIO5 |
| SC CS | GPIO8 |

⚠️ **重要**：请根据实际PCB布线调整上述引脚分配！

## 重要说明

### SPI 通信规则

- 所有 IMU 传感器的 SPI 引脚（MOSI、MISO、SCK）均并联
- CS 引脚单独引出，每次通信只需拉低需要通信的传感器 CS 引脚
- 非通信的传感器 CS 引脚必须保持高电平
- 一次只能和一个传感器通信，否则会异常
- 所有 CS 引脚默认上拉

### 中断引脚规则

- 所有 IMU 传感器的 INT 引脚都连接在一起
- 使用各传感器的 INT1 引脚
- 默认上拉，需要配置传感器 INT1 为开漏模式
- 多传感器都配置中断时无法直接得知是哪个传感器产生的事件
- 需要手动通过 SPI 轮询各传感器的事件寄存器

## 测试原理

每个传感器都有唯一的 WHO_AM_I 寄存器，通过读取该寄存器可以验证传感器是否正确连接。

### 测试流程

1. 初始化 SPI 总线 (10MHz, Mode 0)
2. 配置各传感器的 CS 引脚为高电平（未选中状态）
3. 等待传感器上电稳定 (500ms)
4. 逐个测试传感器：
   - 拉低目标传感器的 CS 引脚
   - 发送读命令（寄存器地址 | 0x80）
   - 读取响应数据
   - 拉高 CS 引脚
   - 验证返回值是否匹配期望值

## 编译和烧录

### 编译

```bash
cargo build -p esp32c3-board --target riscv32imc-unknown-none-elf
```

### 烧录到开发板

使用 probe-rs:

```bash
cargo flash --chip=esp32c3
```

或使用 espressif flash 工具。

### 查看日志

通过 RTT (Real Time Transfer) 查看日志输出：

```bash
probe-rs rtt attach --chip=esp32c3
```

## 当前状态

✅ **项目可成功编译！**

⚠️ **待实现功能**：

当前代码中的 SPI 通信和 GPIO 控制功能使用了占位符，实际使用时需要根据 esp-hal 1.0 的正确 API 实现。

### 需要完成的任务

1. **GPIO 配置**
   - 将 GPIO 引脚配置为输出模式
   - 设置正确的初始电平（高电平 = 未选中）

2. **SPI 初始化**
   - 配置正确的 SPI 引脚（MOSI, MISO, SCK）
   - 设置 SPI 频率（建议 10MHz）
   - 配置 SPI 模式（Mode 0）

3. **SPI 通信实现**
   - 实现正确的 CS 选择逻辑
   - 实现寄存器读写函数
   - 处理通信错误

### 参考 API

由于 esp-hal 1.0 的 API 有较大变动，建议参考：
- [esp-hal GitHub](https://github.com/esp-rs/esp-hal)
- [embedded-hal 文档](https://docs.rs/embedded-hal/)
- [ESP32-C3 数据手册](https://www.espressif.com/sites/default/files/documentation/esp32-c3_datasheet_cn.pdf)

## 示例代码结构

当完成 GPIO 和 SPI 配置后，测试代码结构如下：

```rust
// 选中传感器（拉低CS）
cs_pin.set_low();
Timer::after(Duration::from_micros(10)).await;

// 发送读命令
let read_cmd = whoami_addr | 0x80;
let mut tx_buf = [read_cmd, 0x00];

// 执行 SPI 传输
match spi.transfer(&mut tx_buf) {
    Ok(_) => {
        // 读取 WHO_AM_I 值
        let whoami = tx_buf[1];
        // 取消选中
        cs_pin.set_high();
        // 验证
        if whoami == expected_value {
            info!("[PASS] ...");
        } else {
            info!("[FAIL] ...");
        }
    }
    Err(_) => {
        // 错误处理
        cs_pin.set_high();
        info!("[FAIL] SPI communication error");
    }
}
```

## 故障排除

### 编译错误

如果遇到编译错误，请检查：

1. **SPI API 变化**：esp-hal 1.0 的 API 与旧版本不同
2. **GPIO 配置**：需要正确配置 GPIO 模式和初始状态
3. **泛型参数**：注意泛型参数和 lifetimes 的正确使用

### 运行时错误

如果测试失败：

1. **检查硬件连接**：确认 SPI 和 CS 引脚正确连接
2. **检查电源**：确认 PCB 正确供电（推荐 5V）
3. **检查引脚分配**：确认代码中的 GPIO 编号与实际布线一致
4. **检查传感器状态**：确认传感器已正确焊接到 PCB

## 许可证

根据 [Cargo.toml](Cargo.toml) 中定义的许可证。
