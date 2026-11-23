//! Keeps the key reference in `docs/theming.md` in step with the theme
//! tables: the test regenerates the list and fails, printing the expected
//! text, when the document is out of date.

use super::{BuiltinTheme, Override, Role, ThemeSection};
use crate::design::palette::PaletteKey;

const BEGIN: &str = "<!-- BEGIN GENERATED KEY REFERENCE -->";
const END: &str = "<!-- END GENERATED KEY REFERENCE -->";

fn hex(color: egui::Color32) -> String {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    if a == u8::MAX {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}

fn table(out: &mut String, heading: &str, rows: impl Iterator<Item = (&'static str, String)>) {
    out.push_str(&format!(
        "\n### `{heading}`\n\n| Key | Harbour Dark |\n| --- | --- |\n"
    ));
    for (key, default) in rows {
        out.push_str(&format!("| `{key}` | `{default}` |\n"));
    }
}

/// The generated part of `docs/theming.md`: every key per table with its
/// Harbour Dark value.
fn key_reference() -> String {
    let harbour = BuiltinTheme::HarbourDark.definition();
    let mut out = String::new();
    table(
        &mut out,
        ThemeSection::Palette.table(),
        PaletteKey::ALL
            .iter()
            .map(|entry| (entry.key(), hex(harbour.palette(*entry)))),
    );
    for section in [ThemeSection::Roles, ThemeSection::Terminal] {
        table(
            &mut out,
            section.table(),
            Role::ALL
                .iter()
                .filter(|role| role.section() == section)
                .map(|role| (role.key(), harbour.role(*role).key().to_owned())),
        );
    }
    table(
        &mut out,
        ThemeSection::Components.table(),
        Override::ALL.iter().map(|component| {
            (
                component.key(),
                harbour.component(*component).key().to_owned(),
            )
        }),
    );
    out
}

#[test]
fn theming_doc_lists_every_key() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/theming.md");
    let doc = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let start = doc
        .find(BEGIN)
        .expect("docs/theming.md has the begin marker")
        + BEGIN.len();
    let end = doc.find(END).expect("docs/theming.md has the end marker");
    let expected = key_reference();
    assert!(
        doc[start..end].trim() == expected.trim(),
        "the key reference in docs/theming.md is out of date. Replace everything between \
         `{BEGIN}` and `{END}` with:\n\n{}\n",
        expected.trim()
    );
}
