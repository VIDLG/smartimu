# Hardware

## 当前目标板

当前实现主线面向 5 IMU 测试板和 `ESP32-C3`。

## 传感器

- ICM-42688-HXY
- ICM-42688-PC
- BMI270
- QMI8658A
- SC7U22

## ESP32-C3 引脚

| 功能 | GPIO |
|------|------|
| SPI SCK | GPIO6 |
| SPI MOSI | GPIO7 |
| SPI MISO | GPIO2 |
| ICM-42688-HXY | GPIO1 |
| ICM-42688-PC | GPIO3 |
| BMI270 | GPIO4 |
| QMI8658A | GPIO5 |
| SC7U22 | GPIO10 |

## 总线约束

- 所有 IMU 共享同一 SPI 总线
- 每个 IMU 通过独立目标设备选择线接入
- 任一时刻只允许访问一个 IMU

## 补充资料

- [project-guide.md](project-guide.md)
- [sensor-identification.md](sensor-identification.md)
- [troubleshooting.md](troubleshooting.md)
