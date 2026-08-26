#!/usr/bin/env python3
"""Flash SmartIMU firmware through an explicitly resolved serial port."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

from .serial_open_check import detect_port


class Args(argparse.Namespace):
    def __init__(self) -> None:
        super().__init__()
        self.port: str = ""
        self.image: str = ""


def main() -> int:
    parser = argparse.ArgumentParser(description="Flash ESP32-C3 SmartIMU firmware")
    _ = parser.add_argument(
        "port",
        nargs="?",
        default="",
        help="serial port name; auto-detected when omitted",
    )
    _ = parser.add_argument(
        "image",
        nargs="?",
        default="target/riscv32imc-unknown-none-elf/debug/esp32c3-board",
        help="firmware image path",
    )
    args = parser.parse_args(namespace=Args())

    port = args.port or os.environ.get("ESPFLASH_PORT") or detect_port()
    if not port:
        return 2

    image = Path(args.image)
    if not image.is_file():
        print(f"firmware image not found: {image}", file=sys.stderr)
        return 2

    print(f"flashing {image} to {port}")
    return subprocess.run(
        ["espflash", "flash", "--chip", "esp32c3", "--port", port, str(image)],
        check=False,
    ).returncode


if __name__ == "__main__":
    sys.exit(main())
