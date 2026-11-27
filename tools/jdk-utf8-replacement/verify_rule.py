#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""#190: a second copy of the JDK UTF-8 replacement rule, and the checks of it.

`new String(bytes, StandardCharsets.UTF_8)` of the JDK replaces a malformed
UTF-8 sequence with U+FFFD by the number of bytes its replacing step consumed.
`jdk_malformed_prefix` returns that number, read off `String.decodeUTF8_UTF16`,
`isMalformed3`, `isMalformed3_2`, `malformed3` and `malformed4` of the JDK
source the design names (docs/design/issue-190-bytestring.md, section 8.2),
not off the Rust code:

- a lead byte that never starts a sequence (`0x80..=0xBF`, `0xF5..=0xFF`) and
  a lead byte that cannot start a shortest form (`0xC0`, `0xC1`, `0xF0..=0xF4`
  with a second byte outside the range of the shortest form) consume one byte;
- a two- or three-byte sequence whose second byte is not a continuation byte
  consumes one byte (`malformed3` returns 1), and so does `0xE0` followed by
  `0x80..=0x9F`, whether or not a third byte follows;
- a three-byte sequence whose second byte is a continuation byte in the
  shortest range, and whose third byte is missing or is not a continuation
  byte, consumes two bytes (`malformed3` returns 2);
- a four-byte sequence consumes as many bytes as form its longest prefix that
  still could have started a shortest form (`malformed4`);
- a three-byte sequence that is well formed but decodes to a surrogate (lead
  `0xED`, second byte `0xA0..=0xBF`) consumes three bytes as one U+FFFD.

This is the rule written in another form, so it is not the independent expected
value: that is the table `GenerateJdkUtf8Table.java` takes from the JDK and the
Rust test holds in `crates/fraktor-actor/tests/byte_string_jdk_table.tsv`. This
copy only predicts what the JDK should do before it is run, and then checks
itself against the JDK: on every row of the table, and on the inputs of the
properties the design names, through `JdkOracle.java`.

Usage:
    python3 tools/jdk-utf8-replacement/verify_rule.py --table crates/fraktor-actor/tests/byte_string_jdk_table.tsv \
        --oracle "java tools/jdk-utf8-replacement/JdkOracle.java"
    java tools/jdk-utf8-replacement/GenerateJdkUtf8Table.java  # the table again
"""

from __future__ import annotations

import argparse
import itertools
import shlex
import subprocess
import sys
from pathlib import Path

REPLACEMENT = "\uFFFD"

# The cases the table must hold: (name, bytes). The Rust test has the same
# list; a case added there without being added here fails `--table`.
JDK_CASES = [
    ("empty", b""),
    ("base / bmp two bytes", bytes([0xC3, 0xA9])),
    ("base / bmp three bytes", bytes([0xE2, 0x82, 0xAC])),
    ("base / supplementary four bytes", bytes([0xF0, 0x9F, 0x98, 0x80])),
    ("base / overlong two bytes", bytes([0xC0, 0x80])),
    ("base / overlong two bytes", bytes([0xC1, 0xBF])),
    ("base / overlong three bytes", bytes([0xE0, 0x80, 0x80])),
    ("base / overlong three bytes", bytes([0xE0, 0x9F, 0xBF])),
    ("base / overlong three bytes", bytes([0xE0, 0xA0, 0x80])),
    ("base / overlong four bytes", bytes([0xF0, 0x80, 0x80, 0x80])),
    ("base / overlong four bytes", bytes([0xF0, 0x8F, 0xBF, 0xBF])),
    ("base / overlong four bytes", bytes([0xF0, 0x90, 0x80, 0x80])),
    ("base / above U+10FFFF", bytes([0xF4, 0x90, 0x80, 0x80])),
    ("base / above U+10FFFF", bytes([0xF4, 0x8F, 0xBF, 0xBF])),
    ("base / above U+10FFFF", bytes([0xF5, 0x80, 0x80, 0x80])),
    ("base / cut sequences", bytes([0xC3])),
    ("base / cut sequences", bytes([0xE2, 0x82])),
    ("base / cut sequences", bytes([0xF0, 0x9F, 0x98])),
    ("base / cut sequences", bytes([0xF0, 0x9F, 0x98, 0x41])),
    ("base / lone continuation bytes", bytes([0x80])),
    ("base / lone continuation bytes", bytes([0x80, 0x80])),
    ("base / lone continuation bytes", bytes([0xBF])),
    ("base / bytes that never occur in UTF-8", bytes([0xF5])),
    ("base / bytes that never occur in UTF-8", bytes([0xF8, 0x88, 0x80, 0x80, 0x80])),
    ("base / bytes that never occur in UTF-8", bytes([0xFE, 0xFF])),
    ("base / a three-byte sequence cut before a non-continuation byte", bytes([0xE1, 0x41, 0x80])),
    ("base / a three-byte sequence cut before a non-continuation byte", bytes([0xE2, 0x82, 0x41])),
    ("base / mixed", bytes([0x61, 0x80, 0x62, 0xF0, 0x9F, 0x98, 0x80])),
    ("base / mixed", bytes([0xE1, 0x80, 0xE2, 0xF0, 0x91, 0x92, 0xF1, 0xBF, 0x41])),
    ("ed three bytes / ED A0 80", bytes([0xED, 0xA0, 0x80])),
    ("ed three bytes / ED A0 80", bytes([0xED, 0xA0, 0xBD])),
    ("ed three bytes / ED A0 80", bytes([0xED, 0xAF, 0xBF])),
    ("ed three bytes / ED A0 80", bytes([0xED, 0xB0, 0x80])),
    ("ed three bytes / ED A0 80", bytes([0xED, 0xBF, 0xBF])),
    ("ed three bytes / inside text", bytes([0x61, 0xED, 0xA0, 0x80, 0x62])),
    ("ed three bytes / inside text", bytes([0xC3, 0xA9, 0xED, 0xBF, 0xBF, 0xC3, 0xA9])),
    ("ed three bytes / two in a row", bytes([0xED, 0xA0, 0x80, 0xED, 0xB0, 0x80])),
    ("ed three bytes / two in a row", bytes([0xED, 0xA0, 0x80, 0xED, 0xA0, 0x80])),
    ("ed three bytes / followed by a continuation byte", bytes([0xED, 0xA0, 0x80, 0x80])),
    ("ed three bytes / followed by a continuation byte", bytes([0xED, 0xA0, 0x80, 0x80, 0x80])),
    ("ed three bytes / followed by a continuation byte", bytes([0xED, 0xA0, 0x80, 0x80, 0x80, 0x80])),
    ("ed three bytes / not reached", bytes([0xED, 0xA0, 0x80, 0x80, 0x41])),
    ("ed three bytes / not reached", bytes([0xED, 0xA0, 0x80, 0x80, 0x80, 0x41])),
    ("ed cut at two bytes / ED A0", bytes([0xED, 0xA0])),
    ("ed cut at two bytes / ED A0", bytes([0xED, 0xBF])),
    ("ed cut at two bytes / ED A0", bytes([0x61, 0xED, 0xA0])),
    ("ed cut at two bytes / followed by a continuation byte", bytes([0xED, 0xA0, 0x80])),
    ("ed cut at two bytes / followed by a continuation byte", bytes([0xED, 0xBF, 0x80, 0x80])),
    ("ed three bytes cut before a non-continuation byte / ED A0 41", bytes([0xED, 0xA0, 0x41])),
    ("ed three bytes cut before a non-continuation byte / ED A0 41", bytes([0xED, 0xBF, 0x41])),
    ("ed three bytes cut before a non-continuation byte / ED A0 41", bytes([0xED, 0xA0, 0xC3, 0xA9])),
    ("ed three bytes cut before a non-continuation byte / ED A0 41", bytes([0xED, 0xA0, 0xED, 0xA0, 0x80])),
    ("ed three bytes cut before a non-continuation byte / ED A0 41", bytes([0xED, 0xA0, 0x80, 0xED, 0xBF, 0xBF, 0xED, 0xAF, 0x41])),
    ("ed three bytes cut before a non-continuation byte / followed by a two-byte sequence", bytes([0xED, 0xA0, 0xC3])),
    ("ed three bytes cut before a non-continuation byte / two non-continuation bytes", bytes([0xED, 0xA0, 0xC0])),
    ("ed three bytes cut before a non-continuation byte / two non-continuation bytes", bytes([0xED, 0xA0, 0x00])),
    ("ed three bytes cut before a non-continuation byte / two non-continuation bytes", bytes([0xED, 0xA0, 0x7F])),
    ("ed three bytes cut before a non-continuation byte / two non-continuation bytes", bytes([0xED, 0xA0, 0xF5])),
    ("ed three bytes cut before a non-continuation byte / two non-continuation bytes", bytes([0xED, 0xA0, 0xED])),
    ("ed three bytes cut before a non-continuation byte / three non-continuation bytes", bytes([0xED, 0xA0, 0x41, 0x42])),
    ("ed three bytes cut before a non-continuation byte / three non-continuation bytes", bytes([0xED, 0xA0, 0xC0, 0x80])),
    ("ed three bytes cut at two bytes before a non-continuation byte / ED A0 41", bytes([0xED, 0xBF, 0x41])),
    ("ed three bytes cut at two bytes before a non-continuation byte / ED A0 41", bytes([0xED, 0xA0, 0xC0])),
    ("ed three bytes cut at two bytes before a non-continuation byte / ED A0 41", bytes([0xED, 0xA0, 0x00])),
    ("ed three bytes cut at two bytes before a non-continuation byte / ED A0 41", bytes([0xED, 0xA0, 0x80, 0x41])),
    ("before an ed unit / E1 80 ED A0 80 80", bytes([0xE1, 0x80, 0xED, 0xA0, 0x80, 0x80])),
    ("before an ed unit / E1 80 ED A0 80 80", bytes([0xE0, 0x80, 0x80, 0xED, 0xA0, 0x80])),
    ("before an ed unit / E1 80 ED A0 80 80", bytes([0xF0, 0x9F, 0x98, 0xED, 0xA0, 0x80, 0x80])),
    ("before an ed unit / a cut three-byte sequence", bytes([0xE2, 0x82, 0xED, 0xA0, 0x80])),
    ("before an ed unit / a cut three-byte sequence", bytes([0xE2, 0x82, 0xED, 0xA0])),
    ("before an ed unit / a cut four-byte sequence", bytes([0xF0, 0x9F, 0x98, 0xED, 0xA0, 0x80])),
    ("before an ed unit / a cut four-byte sequence", bytes([0xF0, 0x9F, 0x98, 0xED, 0xA0])),
    ("next to the surrogates / scalars below the surrogates", bytes([0xED, 0x9F, 0xBF])),
    ("next to the surrogates / scalars below the surrogates", bytes([0xED, 0x80, 0x80])),
    ("next to the surrogates / scalars below the surrogates", bytes([0xED, 0x9F, 0x80])),
    ("next to the surrogates / scalars above the surrogates", bytes([0xEE, 0x80, 0x80])),
    ("next to the surrogates / scalars above the surrogates", bytes([0xEE, 0x9F, 0xBF])),
    ("next to the surrogates / the first scalar of each range after an ed-like prefix", bytes([0xED, 0x9F, 0xBF, 0xED, 0xA0, 0x80])),
    ("next to the surrogates / the first scalar of each range after an ed-like prefix", bytes([0xED, 0x9F, 0x80, 0xED, 0x80, 0x80])),
    ("base / the design's examples", bytes([0xED, 0xA0, 0x80])),
    ("base / the design's examples", bytes([0xED, 0xA0])),
    ("base / the design's examples", bytes([0xC0, 0x80])),
    ("base / the design's examples", bytes([0xE0, 0x80, 0x80])),
    ("base / the design's examples", bytes([0xF4, 0x90, 0x80, 0x80])),
    ("base / the design's examples", bytes([0xF0, 0x9F, 0x98])),
    ("base / the design's examples", bytes([0x80])),
    ("base / the design's examples", bytes([0xF5])),
    ("base / the design's examples", bytes([0xFF])),
    ("base / a real U+FFFD is indistinguishable from a replacement", bytes([0xEF, 0xBF, 0xBD])),
    ("base / a real U+FFFD is indistinguishable from a replacement", bytes([0x61, 0xEF, 0xBF, 0xBD, 0x62])),
    ("base / a real U+FFFD is indistinguishable from a replacement", bytes([0x80, 0xEF, 0xBF, 0xBD])),
    ("base / a real U+FFFD is indistinguishable from a replacement", bytes([0xEF, 0xBF, 0xBD, 0xED, 0xA0, 0x80])),
    ("base / intermediate", bytes([0xC3, 0xA9, 0xED, 0xA0, 0x80, 0xC0, 0x80, 0xE0, 0x80, 0x80, 0xF0, 0x9F, 0x98, 0x41, 0x80])),
    ("base / intermediate", bytes([0xF4, 0x90, 0x80, 0x80, 0xED, 0xBF, 0xBF, 0xED, 0x9F, 0xBF, 0xED, 0x80, 0x80, 0xEE, 0x80, 0x80])),
]


def is_continuation(byte: int) -> bool:
    return byte & 0xC0 == 0x80


def shortest_range(lead: int) -> tuple[int, int]:
    """The range of the second byte that a shortest form allows for the lead.

    `isMalformed3` and `isMalformed3_2` name only `0xE0`; a 3-byte sequence
    with any other lead accepts all continuation second bytes, and a surrogate
    is caught after the three bytes are decoded.
    """
    if lead == 0xE0:
        return 0xA0, 0xBF
    if lead == 0xF0:
        return 0x90, 0xBF
    if lead == 0xF4:
        return 0x80, 0x8F
    return 0x80, 0xBF


def jdk_malformed_prefix(data: bytes, at: int) -> tuple[int, int]:
    """(bytes consumed by the unit at `at`, decoded code point or None if replaced).

    Mirrors `String.decodeUTF8_UTF16` / `isMalformed3` / `malformed3` /
    `malformed4` of the JDK, which is the path `new String(bytes, UTF_8)`
    takes. The second item is the code point only when the unit is a well
    formed sequence, so the caller can tell a decoded surrogate from a
    replacement.
    """
    lead = data[at]
    rest = len(data) - at
    if lead < 0x80:
        return 1, lead
    if 0xC2 <= lead <= 0xDF:
        if rest >= 2 and is_continuation(data[at + 1]):
            return 2, (lead & 0x1F) << 6 | data[at + 1] & 0x3F
        return 1, None
    if 0xE0 <= lead <= 0xEF:
        low, high = shortest_range(lead)
        if rest < 2:
            return 1, None
        second = data[at + 1]
        if not is_continuation(second):
            return 1, None
        if rest == 2:
            # `isMalformed3_2`: without a third byte, an `0xE0` lead with a
            # second byte below 0xA0 is one byte and `malformed3` is not run.
            if lead == 0xE0 and 0x80 <= second <= 0x9F:
                return 1, None
            return 2, None
        third = data[at + 2]
        if not low <= second <= high:
            # `malformed3` starts with `(b1 == 0xe0 && (b2 & 0xe0) == 0x80)`.
            if lead == 0xE0 and 0x80 <= second <= 0x9F:
                return 1, None
            return 2, None
        if not is_continuation(third):
            return 2, None
        if lead == 0xED and 0xA0 <= second <= 0xBF:
            # Well formed, but it decodes to a surrogate: the JDK replaces the
            # three bytes as one unit, unlike `from_utf8_lossy`.
            return 3, None
        code_point = (lead & 0x0F) << 12 | (second & 0x3F) << 6 | third & 0x3F
        if 0xD800 <= code_point <= 0xDFFF:
            return 3, None
        return 3, code_point
    if 0xF0 <= lead <= 0xF4:
        low, high = shortest_range(lead)
        if rest == 1:
            return 1, None
        second = data[at + 1]
        if not is_continuation(second) or not low <= second <= high:
            return 1, None
        if rest == 2:
            return 2, None
        if not is_continuation(data[at + 2]):
            return 2, None
        if rest == 3:
            return 3, None
        if not is_continuation(data[at + 3]):
            return 3, None
        code_point = (lead & 0x07) << 18 | (second & 0x3F) << 12 | (data[at + 2] & 0x3F) << 6 | data[at + 3] & 0x3F
        return 4, code_point
    return 1, None


def decode(data: bytes) -> str:
    out = []
    at = 0
    while at < len(data):
        consumed, code_point = jdk_malformed_prefix(data, at)
        out.append(REPLACEMENT if code_point is None else chr(code_point))
        at += consumed
    return "".join(out)


def code_points(text: str) -> str:
    return " ".join(f"U+{ord(c):04X}" for c in text)


def hex_bytes(data: bytes) -> str:
    return " ".join(f"{b:02X}" for b in data)


def read_table(path: Path) -> list[tuple[str, bytes, str]]:
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != 3:
            raise SystemExit(f"{path}: {line!r} is not three tab-separated fields")
        inputs, expected, name = fields
        data = bytes(int(part, 16) for part in inputs.split()) if inputs else b""
        rows.append((name, data, expected))
    return rows


def check_table(path: Path) -> int:
    rows = read_table(path)
    problems = 0
    seen = set()
    for name, data, expected in rows:
        predicted = code_points(decode(data))
        if predicted != expected:
            problems += 1
            print(f"{name}: {hex_bytes(data)}: rule says {predicted}, JDK says {expected}")
        seen.add((data, expected))
    for name, data in JDK_CASES:
        expected = code_points(decode(data))
        if (data, expected) not in seen:
            problems += 1
            print(f"the table lacks {name}: {hex_bytes(data)}, which the rule decodes to {expected}")
    print(f"{len(rows)} cases, {problems} problems")
    return problems


def property_inputs() -> list[tuple[str, bytes]]:
    """The inputs of the properties, each with the claim it makes of the JDK output.

    The claims are about the structure of the JDK output, so they hold for the
    JDK and for any rule that copies it; `--oracle` checks them against the JDK
    itself.
    """
    out: list[tuple[str, bytes]] = []

    # (a) A fourth and further continuation byte is its own unit.
    for tail in range(0x80, 0xC0):
        out.append((f"a fourth byte is its own unit after a surrogate ({tail:02X})", bytes([0xED, 0xA0, 0x80, tail])))
        out.append((f"a fifth byte is its own unit after a scalar ({tail:02X})", bytes([0xF0, 0x9F, 0x98, 0x80, tail])))

    # (b) Two `0xED` units in a row, and `0xED` as the third byte. The first
    # unit always ends before the third byte (`malformed3` returns 2), and the
    # second unit is always one replacement.
    for second in range(0xA0, 0xC0):
        for third in range(0x00, 0x100):
            out.append((f"a cut surrogate unit then 0xED ({second:02X}, {third:02X})", bytes([0xED, second, third, 0xED, second, 0x80])))

    # (c) A third byte that is not a continuation ends the unit at two bytes.
    for second in range(0xA0, 0xC0):
        for third in (0xC0, 0xC1, 0xC2, 0x00, 0x41, 0x7F):
            out.append((f"a third byte that ends the unit ({second:02X}, {third:02X})", bytes([0xED, second, third, 0x80, 0x41])))

    # (d) `0xED` is not a continuation byte, so it cannot be the second or the
    # third byte of a malformed unit: a unit that starts before it ends on it.
    for lead in (0xC2, 0xE0, 0xE1, 0xE2, 0xEF, 0xF0, 0xF4, 0xF5):
        for second in range(0x40, 0xC0):
            out.append((f"a unit before 0xED ({lead:02X}, {second:02X})", bytes([lead, second, 0xED, 0xA0, 0x80])))

    # (e) The scalars next to the surrogates.
    for second in range(0x80, 0xA0):
        for third in range(0x80, 0xC0):
            out.append((f"a scalar below the surrogates ({second:02X}, {third:02X})", bytes([0xED, second, third])))

    # Every input of up to two bytes.
    for length in (0, 1, 2):
        for data in itertools.product(range(0x100), repeat=length):
            out.append(("every input of up to two bytes", bytes(data)))
    return out


def oracle_decode(inputs: list[bytes], oracle: list[str]) -> list[str]:
    """`new String(bytes, UTF_8)` of each input, run through the JDK oracle program."""
    rows = "\n".join(hex_bytes(data) for data in inputs) + "\n"
    done = subprocess.run(oracle, input=rows, capture_output=True, text=True, check=True)
    outputs = done.stdout.splitlines()
    if len(outputs) != len(inputs):
        raise SystemExit(f"the oracle printed {len(outputs)} lines for {len(inputs)} inputs")
    return ["" if line == "-" else line for line in outputs]


def check_properties(limit: int, oracle: list[str] | None) -> int:
    """The properties of the rule, and, with an oracle, the rule against the JDK."""
    problems = 0

    def fail(message: str) -> None:
        nonlocal problems
        problems += 1
        print(message)

    cases = property_inputs()
    step = max(1, len(cases) // limit)
    cases = cases[::step]

    def structural(name: str, data: bytes, output: str, units: list[str]) -> None:
        if name.startswith("a fourth byte") or name.startswith("a fifth byte"):
            if len(units) != 2 or units[1] != "U+FFFD":
                fail(f"{name}: {hex_bytes(data)}: {output}")
        elif name.startswith("a cut surrogate unit"):
            if not units or units[0] != "U+FFFD" or units[-1] != "U+FFFD":
                fail(f"{name}: {hex_bytes(data)}: {output}")
        elif name.startswith("a third byte"):
            # The unit that starts at `0xED` is one replacement; the third
            # byte and what follows decode on their own.
            rest = code_points(decode(data[2:])).split()
            if not units or units[0] != "U+FFFD" or units[1:] != rest:
                fail(f"{name}: {hex_bytes(data)}: {output}")
        elif name.startswith("a unit before 0xED"):
            # Whatever the unit before `0xED` is, its last unit is one
            # replacement for `0xED`, `0xA0`, `0x80`.
            if not units or units[-1] != "U+FFFD":
                fail(f"{name}: {hex_bytes(data)}: {output}")
        elif name.startswith("a scalar below"):
            second, third = data[1], data[2]
            wanted = f"U+{0xD000 + (second - 0x80) * 64 + (third - 0x80):04X}"
            if units != [wanted]:
                fail(f"{name}: {hex_bytes(data)}: {output}, wanted {wanted}")

    if oracle is not None:
        inputs = [data for _, data in cases]
        outputs = oracle_decode(inputs, oracle)
        for (name, data), output in zip(cases, outputs):
            predicted = code_points(decode(data))
            if predicted != output:
                fail(f"{name}: {hex_bytes(data)}: rule says {predicted}, JDK says {output}")
            structural(name, data, output, output.split())
        print(f"{len(cases)} inputs, the rule and the JDK")
    else:
        for name, data in cases:
            output = decode(data)
            structural(name, data, code_points(output), [code_points(c) for c in output])
        print(f"{len(cases)} inputs, the rule alone (pass --oracle for the JDK)")
    print(f"rule properties: {problems} problems")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--table", type=Path, help="a table written by GenerateJdkUtf8Table.java")
    parser.add_argument(
        "--oracle",
        help="the JDK oracle as a command, for example `java tools/jdk-utf8-replacement/JdkOracle.java`",
    )
    parser.add_argument("--limit", type=int, default=20000, help="the most property inputs to run")
    args = parser.parse_args()
    oracle = shlex.split(args.oracle) if args.oracle else None
    problems = check_properties(args.limit, oracle)
    if args.table is not None:
        problems += check_table(args.table)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
