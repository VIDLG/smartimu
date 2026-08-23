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


def candidate_names(port_name: str) -> list[str]:
    candidates = [port_name]
    upper = port_name.upper()
    if upper.startswith("COM"):
        suffix = port_name[3:]
        if suffix.isdigit() and int(suffix) >= 10:
            candidates.append("\\\\.\\" + port_name)
    return candidates


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
        "port", nargs="?", default="COM15", help="serial port name, default: COM15"
    )
    parser.add_argument(
        "baud", nargs="?", type=int, default=115_200, help="baud rate, default: 115200"
    )
    parser.add_argument(
        "--timeout", type=float, default=0.2, help="open/read timeout in seconds"
    )
    args = parser.parse_args()

    print(f"trying to open {args.port} @ {args.baud}")
    return check_port(candidate_names(args.port), args.baud, args.timeout)


if __name__ == "__main__":
    sys.exit(main())
