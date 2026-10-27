#!/usr/bin/env python3
"""Prepare the ignored standalone test lock using only committed dependencies.

Cargo prunes a copy of the consumer lock for the standalone library. Every
resulting package identity and checksum must still occur in the consumer lock.
Compilation, linting and dependency checks keep their --locked arguments.
"""
import json
from pathlib import Path
import re
import subprocess
import sys


def records(text):
    header, *blocks = re.split(r"(?m)^\[\[package\]\]\r?\n", text)
    if not re.search(r"(?m)^version = 4$", header):
        raise ValueError("Expected generated Cargo.lock format version 4")
    result = set()
    for block in blocks:
        fields = []
        for name in ("name", "version", "source", "checksum"):
            values = re.findall(r"(?m)^" + name + r" = (\".*\")$", block)
            if len(values) > 1 or (not values and name in ("name", "version")):
                raise ValueError("Missing or duplicate package field: " + name)
            fields.append(json.loads(values[0]) if values else None)
        record = tuple(fields)
        if record in result:
            raise ValueError("Duplicate package record: " + repr(record))
        result.add(record)
    return result


def prepare(root):
    source = root / "Cargo.lock"
    destination = root / "xoksa-paths/Cargo.lock"
    if destination.is_symlink():
        raise ValueError("Refusing a symlink for the generated standalone lock")
    original = source.read_bytes()
    allowed = records(original.decode())
    destination.write_bytes(original)
    subprocess.run(["cargo", "metadata", "--offline", "--format-version", "1",
                    "--manifest-path", str(root / "xoksa-paths/Cargo.toml")],
                   check=True, stdout=subprocess.DEVNULL)
    generated = records(destination.read_text())
    if not generated or not any(r[0] == "xoksa-paths" and r[2] is None
                                for r in generated):
        raise ValueError("Standalone library missing from generated lock")
    if generated - allowed:
        raise ValueError("Standalone lock would change fixed dependencies: "
                         + repr(sorted(generated - allowed)))
    if source.read_bytes() != original:
        raise ValueError("Consumer Cargo.lock changed during preparation")
    print("Prepared xoksa-paths/Cargo.lock: " + str(len(generated))
          + " package records, all identities/checksums match Cargo.lock")


if __name__ == "__main__":
    try:
        prepare(Path(sys.argv[1]).resolve() if len(sys.argv) == 2
                else Path(__file__).resolve().parent.parent)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit("Cannot prepare standalone library lock: " + str(error))
