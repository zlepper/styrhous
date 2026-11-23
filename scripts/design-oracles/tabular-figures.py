"""Write a "Figtree Tabular" copy of a static Figtree instance.

CSS `font-variant-numeric: tabular-nums` (the boards' `.tnum`) switches on the font's `tnum`
feature. egui cannot apply OpenType features, so the app gets a second family whose cmap maps each
character straight to the glyph `tnum` substitutes for it. Figtree's `tnum` is a single
substitution of the ten digits by their tabular forms (`zero` -> `zero.tf`, ...), so the copy draws
exactly the outlines Chromium draws for tabular figures. The family is renamed so the two are never
confused (the OFL declares no Reserved Font Name for Figtree).

Usage: python3 tabular-figures.py SOURCE.ttf OUTPUT.ttf
"""

import sys

from fontTools.ttLib import TTFont

SOURCE_FAMILY = "Figtree"
FAMILY = "Figtree Tabular"
# Name IDs: 1 family, 4 full name, 16 typographic family; 3 unique ID, 6 PostScript name.
DISPLAY_NAME_IDS = (1, 4, 16)
POSTSCRIPT_NAME_IDS = (3, 6)


def tnum_substitutions(font):
    """The single substitutions of every lookup the `tnum` feature uses."""
    gsub = font["GSUB"].table
    mapping = {}
    for record in gsub.FeatureList.FeatureRecord:
        if record.FeatureTag != "tnum":
            continue
        for index in record.Feature.LookupListIndex:
            lookup = gsub.LookupList.Lookup[index]
            for subtable in lookup.SubTable:
                if lookup.LookupType == 7:
                    subtable = subtable.ExtSubTable
                if not hasattr(subtable, "mapping"):
                    sys.exit(f"tnum lookup {index} is not a single substitution")
                mapping.update(subtable.mapping)
    if not mapping:
        sys.exit("the font has no tnum substitutions")
    return mapping


def main(source, output):
    font = TTFont(source)
    mapping = tnum_substitutions(font)
    remapped = 0
    for table in font["cmap"].tables:
        if not table.isUnicode():
            continue
        for codepoint, glyph in list(table.cmap.items()):
            if glyph in mapping:
                table.cmap[codepoint] = mapping[glyph]
                remapped += 1
    if remapped == 0:
        sys.exit("no character maps to a glyph tnum substitutes")

    for record in font["name"].names:
        text = record.toUnicode()
        if record.nameID in DISPLAY_NAME_IDS:
            renamed = text.replace(SOURCE_FAMILY, FAMILY, 1)
        elif record.nameID in POSTSCRIPT_NAME_IDS:
            renamed = text.replace(SOURCE_FAMILY, FAMILY.replace(" ", ""), 1)
        else:
            continue
        if renamed == text:
            sys.exit(f"name {record.nameID} ({text!r}) does not contain {SOURCE_FAMILY!r}")
        record.string = renamed
    font.save(output)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
