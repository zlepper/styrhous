"""Prints the codepoints the fallback font must provide, as a CSS unicode-range list.

Usage: python3 -I fallback-codepoints.py BOARD_SRC_DIR FALLBACK_FONT PRIMARY_FONT...

Collects every character in the text of the extracted boards (BOARD_SRC_DIR/*.html) and keeps those
that at least one primary Harbour font lacks: a run in that family falls back for them. Exits 1 if
the fallback font lacks one of them as well, so the gap is caught before rendering.
"""

import html
import re
import sys
from pathlib import Path

from fontTools.ttLib import TTFont


def board_characters(src_dir: Path) -> set[str]:
    chars: set[str] = set()
    for board in sorted(src_dir.glob("*.html")):
        markup = board.read_text(encoding="utf-8")
        markup = re.sub(r"<(style|script)\b.*?</\1>", "", markup, flags=re.S | re.I)
        text = html.unescape(re.sub(r"<[^>]*>", "", markup))
        chars.update(ch for ch in text if not ch.isspace())
    return chars


def main() -> int:
    if len(sys.argv) < 4:
        print(__doc__, file=sys.stderr)
        return 2
    src_dir, fallback, primaries = Path(sys.argv[1]), sys.argv[2], sys.argv[3:]
    primary_cmaps = [set(TTFont(path).getBestCmap()) for path in primaries]
    fallback_cmap = set(TTFont(fallback).getBestCmap())

    needed = sorted(
        ord(ch)
        for ch in board_characters(src_dir)
        if any(ord(ch) not in cmap for cmap in primary_cmaps)
    )
    uncovered = [cp for cp in needed if cp not in fallback_cmap]
    if uncovered:
        listed = ", ".join(f"{chr(cp)} U+{cp:04X}" for cp in uncovered)
        print(f"fallback font lacks characters the boards use: {listed}", file=sys.stderr)
        return 1
    print(", ".join(f"U+{cp:04X}" for cp in needed))
    return 0


if __name__ == "__main__":
    sys.exit(main())
