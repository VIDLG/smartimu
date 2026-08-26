#!/usr/bin/env python3
"""Check whether a serial port can be opened.

This is intentionally a small host-side helper managed by Pixi instead of a
Rust workspace package. It is useful for diagnosing Windows COM port naming and
locking issues before launching the viewer or flashing firmware.
"""

from __future__ import annotations

import argparse
import sys
from collections.abc import Iterable


ESPRESSIF_USB_VID = 0x303A


def candidate_names(port_name: str) -> list[str]:
    candidates = [port_name]
    upper = port_name.upper()
    if upper.startswith("COM"):
        suffix = port_name[3:]
        if suffix.isdigit() and int(suffix) >= 10:
            candidates.append("\\\\.\\" + port_name)
    return candidates


def detect_port() -> str | None:
    from serial.tools import list_ports  # type: ignore[import-not-found]

    ports = list(list_ports.comports())
    espressif_ports = [port for port in ports if port.vid == ESPRESSIF_USB_VID]

    if len(espressif_ports) == 1:
        return espressif_ports[0].device
    if len(ports) == 1:
        return ports[0].device

    if not ports:
        print("no serial ports found")
    else:
        print("could not select one serial port; available ports:")
        for port in ports:
            description = port.description or "unknown device"
            print(f"  {port.device}: {description}")
        print("provide a port explicitly or set ESPFLASH_PORT")
    return None


def check_port(candidates: Iterable[str], baud_rate: int, timeout: float) -> int:
    import serial  # type: ignore[import-not-found]

    for candidate in candidates:
        print(f"candidate: {candidate}")
        try:
            with serial.Serial(candidate, baud_rate, timeout=timeout):
                print(f"OPEN OK: {candidate}")
                return 0
        except serial.SerialException as error:
            print(f"OPEN FAIL: {candidate} => {error}")
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description="Check if a serial port can be opened")
    parser.add_argument(
        "port",
        nargs="?",
        default="",
        help="serial port name; auto-detected when omitted",
    )
    parser.add_argument(
        "baud", nargs="?", type=int, default=115_200, help="baud rate, default: 115200"
    )
    parser.add_argument(
        "--timeout", type=float, default=0.2, help="open/read timeout in seconds"
    )
    args = parser.parse_args()

    port = args.port or detect_port()
    if port is None:
        return 2

    print(f"trying to open {port} @ {args.baud}")
    return check_port(candidate_names(port), args.baud, args.timeout)


if __name__ == "__main__":
    sys.exit(main())
