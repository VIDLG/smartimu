#!/usr/bin/env python3
"""Install the pinned ESP32-C3 Rust target into the active Pixi sysroot."""

from __future__ import annotations

import argparse
import hashlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path, PurePosixPath
from typing import BinaryIO, cast

RUST_VERSION = "1.88.0"
TARGET = "riscv32imc-unknown-none-elf"
ARCHIVE_URL = (
    "https://static.rust-lang.org/dist/2025-06-26/"
    f"rust-std-{RUST_VERSION}-{TARGET}.tar.gz"
)
ARCHIVE_SHA256 = "f45f00156a9f7b82fed22d67fa760d796deba54c883f56a99a9c706f402b600e"


def rust_sysroot() -> Path:
    output = subprocess.check_output(
        ["rustc", "--print", "sysroot"], text=True, encoding="utf-8"
    )
    return Path(output.strip())


def target_dir(sysroot: Path) -> Path:
    return sysroot / "lib" / "rustlib" / TARGET


def is_installed(path: Path) -> bool:
    return path.is_dir() and any((path / "lib").glob("libcore-*.rlib"))


def download_archive(destination: Path) -> None:
    digest = hashlib.sha256()
    request = urllib.request.Request(
        ARCHIVE_URL, headers={"User-Agent": "smartimu-pixi-bootstrap/1"}
    )
    response_stream = cast(BinaryIO, urllib.request.urlopen(request, timeout=60))
    with response_stream as response, destination.open("wb") as archive:
        while chunk := response.read(1024 * 1024):
            _ = archive.write(chunk)
            digest.update(chunk)

    actual_sha256 = digest.hexdigest()
    if actual_sha256 != ARCHIVE_SHA256:
        raise RuntimeError(
            f"Rust target archive SHA256 mismatch: expected {ARCHIVE_SHA256}, got {actual_sha256}"
        )


def extract_target(archive_path: Path, destination: Path) -> None:
    marker = ("lib", "rustlib", TARGET)
    extracted_files = 0

    with tarfile.open(archive_path, mode="r:gz") as archive:
        for member in archive.getmembers():
            parts = PurePosixPath(member.name).parts
            marker_index = next(
                (
                    index
                    for index in range(len(parts) - len(marker) + 1)
                    if parts[index : index + len(marker)] == marker
                ),
                None,
            )
            if marker_index is None:
                continue

            relative_parts = parts[marker_index + len(marker) :]
            if not relative_parts:
                continue
            if any(part in ("", ".", "..") for part in relative_parts):
                raise RuntimeError(f"Unsafe archive member: {member.name}")

            output_path = destination.joinpath(*relative_parts)
            if member.isdir():
                output_path.mkdir(parents=True, exist_ok=True)
                continue
            if not member.isfile():
                raise RuntimeError(f"Unsupported archive member: {member.name}")

            source = archive.extractfile(member)
            if source is None:
                raise RuntimeError(f"Could not read archive member: {member.name}")
            output_path.parent.mkdir(parents=True, exist_ok=True)
            with source, output_path.open("wb") as output:
                shutil.copyfileobj(source, output)
            extracted_files += 1

    if extracted_files == 0 or not is_installed(destination):
        raise RuntimeError(f"Archive did not contain a valid {TARGET} Rust target")


def install() -> Path:
    sysroot = rust_sysroot()
    installed_path = target_dir(sysroot)
    if is_installed(installed_path):
        print(f"Rust target already installed: {installed_path}")
        return installed_path

    rustlib_dir = installed_path.parent
    rustlib_dir.mkdir(parents=True, exist_ok=True)

    print(f"Downloading Rust {RUST_VERSION} target {TARGET}...")
    with tempfile.TemporaryDirectory(prefix="smartimu-rust-target-") as download_dir:
        archive_path = Path(download_dir) / "rust-target.tar.gz"
        download_archive(archive_path)

        with tempfile.TemporaryDirectory(
            prefix=f".{TARGET}-", dir=rustlib_dir
        ) as staging_root:
            staging_path = Path(staging_root) / TARGET
            staging_path.mkdir()
            extract_target(archive_path, staging_path)

            if installed_path.exists():
                shutil.rmtree(installed_path)
            _ = staging_path.replace(installed_path)

    print(f"Installed Rust target: {installed_path}")
    return installed_path


def main() -> int:
    parser = argparse.ArgumentParser(
        description=f"Install Rust {RUST_VERSION} target {TARGET} into the active sysroot"
    )
    _ = parser.add_argument(
        "--check", action="store_true", help="only verify that the target is installed"
    )
    _ = parser.parse_args()
    check_only = "--check" in sys.argv[1:]

    installed_path = target_dir(rust_sysroot())
    if check_only:
        if not is_installed(installed_path):
            print(f"Rust target is not installed: {installed_path}", file=sys.stderr)
            return 1
        print(f"Rust target installed: {installed_path}")
        return 0

    _ = install()
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"error: {error}", file=sys.stderr)
        sys.exit(1)
