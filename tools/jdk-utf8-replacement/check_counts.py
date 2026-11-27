#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""#190: the check behind "the JDK replacement rule was compared on the inputs below".

The design (docs/design/issue-190-bytestring.md, section 8.2) claims the rule
of `ByteString::decode_string` was compared with `new String(bytes, UTF_8)`
of the JDK on whole families of inputs. This script makes those inputs from
the same fixed seeds every time, decodes them with the JDK on one side and
with two Rust programs on the other, and prints the counts it compared:

    uv run python3 tools/jdk-utf8-replacement/check_counts.py

The Rust side needs no build of the crate: `counts_decode.rs` carries
`decode_string` as `crates/fraktor-actor/src/kernel/byte_string/byte_string_value.rs`
has it (copied unchanged but for the indentation, so that the check does not
depend on the rest of the kernel), and `counts_lossy.rs` calls
`String::from_utf8_lossy`. The counts show where the two agree with the JDK and
how many inputs the exception of section 8.2 actually changes. A change to the
crate's `decode_string` must be copied to `counts_decode.rs`; the test
`decode_string_matches_the_pinned_jdk_table` is what pins the crate itself.

The families (`--family` picks one):
  lead-high  every input whose first byte is `0x80..=0xFF` and whose length is
             1 to 3: 128 + 16,384 + 2,097,152 = 2,113,664 inputs, all of them;
  all-bytes  every input of three bytes: 256**3 = 16,777,216 inputs, all of
             them;
  targeted   the rows of the table, `0xED` with every second and third byte in
             rows of three and four bytes, and those units between a prefix and
             a suffix of every kind: the cells of the rule a three-byte row
             cannot reach on its own;
  random     rows of length 0 to 64 drawn from SplitMix64 with the fixed seed
             below, half of them with every byte drawn from `0xED`,
             `0x80..=0xBF`, `0xC0..=0xFF` and ASCII so that the rule fires often.

Every family writes its inputs as hex, one per line, and the JDK program
`JdkUtf8Table.java` answers with the code points. An input whose output is not
the same on both sides of the comparison is printed with its seed and position,
so it repeats. The run ends with the counts and the SHA-256 of each family.
"""

from __future__ import annotations

import argparse
import hashlib
import shutil
import itertools
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / "tools" / "jdk-utf8-replacement"
RUST_DECODE = TOOLS / "counts_decode.rs"
RUST_LOSSY = TOOLS / "counts_lossy.rs"
JDK_TABLE = TOOLS / "JdkUtf8Table.java"
TABLE = ROOT / "crates" / "fraktor-actor" / "tests" / "byte_string_jdk_table.tsv"

# The seeds of the random families: the number and the SplitMix64 state.
BIN_DIR = Path(tempfile.mkdtemp(prefix="counts-190-"))

RANDOM_SEED = 0x1905_B17E_57A1_BCEF
TARGETED_SEED = 0x1905_ED5A_7C6A_1101


class SplitMix64:
    """The generator the test file uses (`std::SplitMix64` in byte_string_spec.rs)."""

    def __init__(self, seed: int) -> None:
        self.state = seed

    def next_u64(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & 0xFFFF_FFFF_FFFF_FFFF
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & 0xFFFF_FFFF_FFFF_FFFF
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & 0xFFFF_FFFF_FFFF_FFFF
        return z ^ (z >> 31)

    def below(self, bound: int) -> int:
        return self.next_u64() % bound

    def byte(self) -> int:
        return self.next_u64() & 0xFF


def family_lead_high() -> list[bytes]:
    """Every input whose first byte is `0x80..=0xFF`, of length 1 to 3.

    128 + 128**2 + 128**3 = 128 + 16,384 + 2,097,152 = 2,113,664 inputs, all of
    them. These are the inputs that start with a byte that is not ASCII, which
    is where every malformed unit of the JDK rule begins.
    """
    out: list[bytes] = []
    for length in (1, 2, 3):
        out.extend(bytes(data) for data in itertools.product(range(0x80, 0x100), repeat=length))
    return out


def family_all_bytes() -> list[bytes]:
    """Every input of three bytes, 256**3 = 16,777,216 of them, all of them.

    This holds the family the review put at 8,421,504 inputs (see the design,
    section 8.2): no input of three bytes is left untried, whatever it starts
    with, so the rule is checked together with every prefix, cut and
    continuation around it.
    """
    out: list[bytes] = []
    for data in itertools.product(range(0x100), repeat=3):
        out.append(bytes(data))
    return out


def family_targeted() -> list[bytes]:
    out: list[bytes] = []
    # The rows of the table, which name every case the design gives.
    for line in TABLE.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        inputs = line.split("\t")[0]
        out.append(bytes(int(part, 16) for part in inputs.split()) if inputs else b"")
    # `0xED` at every position of every row of three and four bytes, so that
    # every second and third byte the rule can read is tried.
    for second in range(0x00, 0x100):
        for third in range(0x00, 0x100):
            out.append(bytes([0xED, second, third]))
            out.append(bytes([0xED, second, third, 0x80]))
            out.append(bytes([0xED, second, 0x80, third]))
    # A prefix and a suffix of every kind around the three bytes of the unit,
    # so that a unit that starts after another one and one that ends before a
    # fourth byte that is not a continuation are both tried.
    prefix = [b"", b"\x41", b"\xed", b"\xed\xa0", b"\xc3", b"\xe2", b"\xf0", b"\xf0\x9f"]
    suffix = [b"", b"\x80", b"\x9f", b"\xa0", b"\xbf", b"\xc0", b"\xed", b"\xed\xa0\x80", b"\x41"]
    for before in prefix:
        for after in suffix:
            for second in (0x00, 0x9F, 0xA0, 0xAF, 0xBF, 0xED, 0x41):
                for third in (0x00, 0x7F, 0x80, 0x9F, 0xA0, 0xBF, 0xC0, 0xED):
                    out.append(before + bytes([0xED, second, third]) + after)
    return out


def family_random(count: int) -> tuple[list[bytes], list[bytes]]:
    """`count` rows of length 0..=64, and the same count of rows that aim at the rule."""
    uniform: list[bytes] = []
    aimed: list[bytes] = []
    for i in range(count):
        generator = SplitMix64(RANDOM_SEED ^ (i * 0xD1B5_4A32_D192_ED03))
        length = generator.below(65)
        uniform.append(bytes(generator.byte() for _ in range(length)))
        generator = SplitMix64(TARGETED_SEED ^ (i * 0xD1B5_4A32_D192_ED03))
        length = generator.below(65)
        row = bytearray()
        for _ in range(length):
            row.append(generator.byte() if generator.below(2) == 0 else _aimed_byte(generator))
        aimed.append(bytes(row))
    return uniform, aimed


def _aimed_byte(generator: SplitMix64) -> int:
    match generator.below(4):
        case 0:
            return 0xED
        case 1:
            return 0x80 + generator.below(0x40)
        case 2:
            return 0xC0 + generator.below(0x40)
        case _:
            return 0x41 + generator.below(0x1A)


def hex_bytes(data: bytes) -> str:
    return " ".join(f"{b:02X}" for b in data)


def build(program: Path) -> Path:
    binary = BIN_DIR / program.stem
    if not binary.exists():
        subprocess.run(["rustc", "-O", "--edition", "2021", "-o", str(binary), str(program)],
                       check=True, capture_output=True, text=True)
    return binary


def run_rust(program: Path, inputs: list[bytes]) -> list[str]:
    rows = "".join(hex_bytes(data) + "\n" for data in inputs)
    done = subprocess.run([str(build(program))], input=rows, capture_output=True, text=True, check=True)
    return done.stdout.splitlines()


def run_jdk(inputs: list[bytes]) -> list[str]:
    rows = "".join(hex_bytes(data) + "\n" for data in inputs)
    done = subprocess.run(["java", str(JDK_TABLE)], input=rows, capture_output=True, text=True, check=True)
    return done.stdout.splitlines()


def compare(name: str, inputs: list[bytes]) -> bool:
    """Run one family through the JDK and both Rust programs; True when all agree."""
    digest = hashlib.sha256("\n".join(hex_bytes(data) for data in inputs).encode()).hexdigest()
    rust = run_rust(RUST_DECODE, inputs)
    lossy = run_rust(RUST_LOSSY, inputs)
    jdk = run_jdk(inputs)
    if not len(inputs) == len(rust) == len(lossy) == len(jdk):
        raise SystemExit(f"{name}: {len(inputs)} inputs, {len(rust)} Rust, {len(lossy)} lossy, {len(jdk)} JDK rows")
    differing = [i for i in range(len(inputs)) if rust[i] != jdk[i]]
    changed = sum(1 for i in range(len(inputs)) if rust[i] != lossy[i])
    print(f"{name}: {len(inputs)} inputs, sha256 {digest}, {len(differing)} differ from the JDK, "
          f"{changed} differ from from_utf8_lossy")
    for i in differing[:10]:
        print(f"  {hex_bytes(inputs[i])}: decode_string {rust[i]}, JDK {jdk[i]}, lossy {lossy[i]}")
    return not differing


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    families = ("lead-high", "all-bytes", "targeted", "random", "everything")
    parser.add_argument("--family", choices=families, default="everything")
    parser.add_argument("--random-count", type=int, default=100_000, help="rows per random family")
    args = parser.parse_args()
    wanted = ("lead-high", "all-bytes", "targeted", "random") if args.family == "everything" else (args.family,)
    ok = True
    try:
        if "lead-high" in wanted:
            ok &= compare("lead-high", family_lead_high())
        if "all-bytes" in wanted:
            ok &= compare("all-bytes", family_all_bytes())
        if "targeted" in wanted:
            ok &= compare("targeted", family_targeted())
        if "random" in wanted:
            uniform, aimed = family_random(args.random_count)
            ok &= compare("random uniform", uniform)
            ok &= compare("random aimed", aimed)
    finally:
        shutil.rmtree(BIN_DIR, ignore_errors=True)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
