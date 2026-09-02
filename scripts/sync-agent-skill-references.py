#!/usr/bin/env python3
"""Generate Local Browser Bridge skill references from the canonical protocol."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import sys
import tempfile


REPO_ROOT = Path(__file__).resolve().parent.parent
SOURCE = REPO_ROOT / "docs" / "internals" / "PROTOCOL.md"
REFERENCE_DIR = REPO_ROOT / "skills" / "local-browser-bridge" / "references"
BOUNDARIES = (
    ("transport.md", b"# Bridge protocol\n"),
    ("browser.md", b"## Browser-control lease model\n"),
    ("computer.md", b"## Native computer commands\n"),
    ("http.md", b"## REST API\n"),
)


def expected_references() -> dict[str, bytes]:
    source = SOURCE.read_bytes()
    try:
        source.decode("utf-8")
    except UnicodeDecodeError as error:
        raise RuntimeError("docs/internals/PROTOCOL.md must be UTF-8") from error
    if b"\r\n" in source or not source.endswith(b"\n"):
        raise RuntimeError("docs/internals/PROTOCOL.md must use LF endings and end with one newline")

    offsets: list[int] = []
    for _, marker in BOUNDARIES:
        if source.count(marker) != 1:
            raise RuntimeError(f"protocol boundary must occur exactly once: {marker!r}")
        offsets.append(source.index(marker))
    if offsets[0] != 0 or offsets != sorted(offsets):
        raise RuntimeError("protocol boundaries are missing or out of order")

    expected: dict[str, bytes] = {}
    for index, (name, _) in enumerate(BOUNDARIES):
        end = offsets[index + 1] if index + 1 < len(offsets) else len(source)
        expected[name] = source[offsets[index] : end]
    if b"".join(expected.values()) != source:
        raise RuntimeError("generated skill references do not reconstruct the protocol exactly")
    return expected


def replace_file(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary_path, path)
    finally:
        temporary_path.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail if a generated reference differs from docs/internals/PROTOCOL.md",
    )
    args = parser.parse_args()

    expected = expected_references()
    stale = [
        name
        for name, data in expected.items()
        if not (REFERENCE_DIR / name).is_file()
        or (REFERENCE_DIR / name).read_bytes() != data
    ]
    unexpected = sorted(
        path.name
        for path in REFERENCE_DIR.glob("*.md")
        if path.name not in expected
    )

    if args.check:
        if stale or unexpected:
            for name in stale:
                print(f"stale or missing generated skill reference: {name}", file=sys.stderr)
            for name in unexpected:
                print(f"unexpected generated skill reference: {name}", file=sys.stderr)
            return 1
        print("Agent skill protocol references are synchronized.")
        return 0

    for name, data in expected.items():
        replace_file(REFERENCE_DIR / name, data)
    for name in unexpected:
        (REFERENCE_DIR / name).unlink()
    print(f"Generated {len(expected)} agent skill protocol references.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
