//! The Harbour font set and its registration with egui.
//!
//! egui cannot select a weight or an optical size inside a family, so every
//! (typeface, weight[, optical size]) pair is registered as its own named
//! family ([`family`]). Every family ends with the same fallbacks: the DejaVu
//! Sans subset that holds the few symbols the design boards use that the
//! Harbour faces lack (↵ ⌘ ⌫), the complete DejaVu Sans (DejaVu Sans Mono for
//! monospace families) for every other symbol and script (● ✓, Cyrillic,
//! Greek), then egui's emoji fonts. The subset comes first, so board text
//! never reaches the complete faces. Bold is never synthesised: every face
//! is a real static font file.
//!
//! The Harbour faces and the subset are the exact static instances the
//! design oracles render with (`scripts/design-oracles/instance-fonts.sh`),
//! so Chromium and egui rasterize identical outlines. The complete DejaVu
//! faces are the unmodified files of the pinned nixpkgs `dejavu_fonts`.
//!
//! [`install`] registers everything; [`super::theme::configure_default`] calls
//! it before the first frame.

use std::sync::{Arc, LazyLock};

use egui::{Context, FontData, FontDefinitions, FontFamily};

/// Names of the registered font families.
pub mod family {
    pub const FIGTREE_REGULAR: &str = "Figtree";
    pub const FIGTREE_MEDIUM: &str = "Figtree Medium";
    pub const FIGTREE_SEMIBOLD: &str = "Figtree SemiBold";
    pub const FIGTREE_BOLD: &str = "Figtree Bold";
    pub const FIGTREE_TABULAR_REGULAR: &str = "Figtree Tabular";
    pub const FIGTREE_TABULAR_MEDIUM: &str = "Figtree Tabular Medium";
    pub const FIGTREE_TABULAR_SEMIBOLD: &str = "Figtree Tabular SemiBold";
    pub const FIGTREE_TABULAR_BOLD: &str = "Figtree Tabular Bold";
    pub const MONO_REGULAR: &str = "IBM Plex Mono";
    pub const MONO_MEDIUM: &str = "IBM Plex Mono Medium";
    pub const MONO_SEMIBOLD: &str = "IBM Plex Mono SemiBold";
    pub const CAPTION_MEDIUM: &str = "IBM Plex Sans Condensed Medium";
    pub const CAPTION_SEMIBOLD: &str = "IBM Plex Sans Condensed SemiBold";
}

/// Vertical font metrics in em units: the values both Chromium and egui read
/// (`hhea`, which equals `OS/2` typo metrics wherever `USE_TYPO_METRICS` is
/// set). The line gap is zero for every Harbour face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceMetrics {
    /// Ascender above the baseline.
    pub ascent: f32,
    /// Descender below the baseline, as a positive number.
    pub descent: f32,
}

/// One embedded font file.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    /// Key of the font data in [`FontDefinitions::font_data`].
    pub key: &'static str,
    /// The registered family that uses this face as its primary font.
    pub family: &'static str,
    pub bytes: &'static [u8],
    pub metrics: FaceMetrics,
}

const FIGTREE_METRICS: FaceMetrics = FaceMetrics {
    ascent: 0.95,
    descent: 0.25,
};
const PLEX_METRICS: FaceMetrics = FaceMetrics {
    ascent: 1.025,
    descent: 0.275,
};
const BRICOLAGE_METRICS: FaceMetrics = FaceMetrics {
    ascent: 0.93,
    descent: 0.27,
};

macro_rules! face {
    ($key:literal, $family:expr, $path:literal, $metrics:expr) => {
        Face {
            key: $key,
            family: $family,
            bytes: include_bytes!(concat!("../../assets/fonts/", $path)),
            metrics: $metrics,
        }
    };
}

pub const FIGTREE_REGULAR: Face = face!(
    "figtree-regular",
    family::FIGTREE_REGULAR,
    "figtree/Figtree-Regular.ttf",
    FIGTREE_METRICS
);
pub const FIGTREE_MEDIUM: Face = face!(
    "figtree-medium",
    family::FIGTREE_MEDIUM,
    "figtree/Figtree-Medium.ttf",
    FIGTREE_METRICS
);
pub const FIGTREE_SEMIBOLD: Face = face!(
    "figtree-semibold",
    family::FIGTREE_SEMIBOLD,
    "figtree/Figtree-SemiBold.ttf",
    FIGTREE_METRICS
);
pub const FIGTREE_BOLD: Face = face!(
    "figtree-bold",
    family::FIGTREE_BOLD,
    "figtree/Figtree-Bold.ttf",
    FIGTREE_METRICS
);
/// Figtree with tabular figures: each weight's digits are the glyphs its
/// `tnum` feature substitutes (CSS `font-variant-numeric: tabular-nums`),
/// since egui cannot apply OpenType features. Written by
/// `scripts/design-oracles/tabular-figures.py`; see [`super::type_scale::TextRole::tabular`].
pub const FIGTREE_TABULAR_REGULAR: Face = face!(
    "figtree-tabular-regular",
    family::FIGTREE_TABULAR_REGULAR,
    "figtree/FigtreeTabular-Regular.ttf",
    FIGTREE_METRICS
);
pub const FIGTREE_TABULAR_MEDIUM: Face = face!(
    "figtree-tabular-medium",
    family::FIGTREE_TABULAR_MEDIUM,
    "figtree/FigtreeTabular-Medium.ttf",
    FIGTREE_METRICS
);
pub const FIGTREE_TABULAR_SEMIBOLD: Face = face!(
    "figtree-tabular-semibold",
    family::FIGTREE_TABULAR_SEMIBOLD,
    "figtree/FigtreeTabular-SemiBold.ttf",
    FIGTREE_METRICS
);
pub const FIGTREE_TABULAR_BOLD: Face = face!(
    "figtree-tabular-bold",
    family::FIGTREE_TABULAR_BOLD,
    "figtree/FigtreeTabular-Bold.ttf",
    FIGTREE_METRICS
);
pub const MONO_REGULAR: Face = face!(
    "plex-mono-regular",
    family::MONO_REGULAR,
    "ibmplexmono/IBMPlexMono-Regular.ttf",
    PLEX_METRICS
);
pub const MONO_MEDIUM: Face = face!(
    "plex-mono-medium",
    family::MONO_MEDIUM,
    "ibmplexmono/IBMPlexMono-Medium.ttf",
    PLEX_METRICS
);
pub const MONO_SEMIBOLD: Face = face!(
    "plex-mono-semibold",
    family::MONO_SEMIBOLD,
    "ibmplexmono/IBMPlexMono-SemiBold.ttf",
    PLEX_METRICS
);
pub const CAPTION_MEDIUM: Face = face!(
    "plex-sans-condensed-medium",
    family::CAPTION_MEDIUM,
    "ibmplexsanscondensed/IBMPlexSansCondensed-Medium.ttf",
    PLEX_METRICS
);
pub const CAPTION_SEMIBOLD: Face = face!(
    "plex-sans-condensed-semibold",
    family::CAPTION_SEMIBOLD,
    "ibmplexsanscondensed/IBMPlexSansCondensed-SemiBold.ttf",
    PLEX_METRICS
);

/// Bricolage Grotesque Bold, one static instance per optical size.
///
/// Chromium sizes Bricolage optically (`opsz` = font size in px), so each
/// display size the design uses has its own instance and family. Sizes are
/// whole pixels; [`bricolage`] maps a font size onto its instance.
pub const BRICOLAGE_BOLD: [Face; 7] = [
    face!(
        "bricolage-bold-opsz13",
        "Bricolage Grotesque Bold opsz13",
        "bricolagegrotesque/BricolageGrotesque-Bold-opsz13.ttf",
        BRICOLAGE_METRICS
    ),
    face!(
        "bricolage-bold-opsz15",
        "Bricolage Grotesque Bold opsz15",
        "bricolagegrotesque/BricolageGrotesque-Bold-opsz15.ttf",
        BRICOLAGE_METRICS
    ),
    face!(
        "bricolage-bold-opsz16",
        "Bricolage Grotesque Bold opsz16",
        "bricolagegrotesque/BricolageGrotesque-Bold-opsz16.ttf",
        BRICOLAGE_METRICS
    ),
    face!(
        "bricolage-bold-opsz18",
        "Bricolage Grotesque Bold opsz18",
        "bricolagegrotesque/BricolageGrotesque-Bold-opsz18.ttf",
        BRICOLAGE_METRICS
    ),
    face!(
        "bricolage-bold-opsz20",
        "Bricolage Grotesque Bold opsz20",
        "bricolagegrotesque/BricolageGrotesque-Bold-opsz20.ttf",
        BRICOLAGE_METRICS
    ),
    face!(
        "bricolage-bold-opsz26",
        "Bricolage Grotesque Bold opsz26",
        "bricolagegrotesque/BricolageGrotesque-Bold-opsz26.ttf",
        BRICOLAGE_METRICS
    ),
    face!(
        "bricolage-bold-opsz28",
        "Bricolage Grotesque Bold opsz28",
        "bricolagegrotesque/BricolageGrotesque-Bold-opsz28.ttf",
        BRICOLAGE_METRICS
    ),
];

/// Optical sizes with a Bricolage instance, in [`BRICOLAGE_BOLD`] order.
const BRICOLAGE_OPSZ: [u8; 7] = [13, 15, 16, 18, 20, 26, 28];

/// The Bricolage Grotesque Bold instance for `size`.
///
/// `const`, so a role constant with a display size that has no instance
/// fails to compile. At run time a missing size panics; add the size to
/// `scripts/design-oracles/instance-fonts.sh` and [`BRICOLAGE_BOLD`].
pub const fn bricolage(size: f32) -> &'static Face {
    let mut index = 0;
    while index < BRICOLAGE_OPSZ.len() {
        if BRICOLAGE_OPSZ[index] as f32 == size {
            return &BRICOLAGE_BOLD[index];
        }
        index += 1;
    }
    panic!("no Bricolage Grotesque instance for this display size");
}

const DEJAVU_METRICS: FaceMetrics = FaceMetrics {
    ascent: 1901.0 / 2048.0,
    descent: 483.0 / 2048.0,
};

/// The first fallback, for the few symbols the design boards use that no
/// Harbour face has (↵ ⌘ ⌫): a DejaVu Sans subset, shared with the oracle
/// renders so those glyphs match Chromium.
pub const FALLBACK: Face = face!(
    "harbour-fallback",
    "Harbour Fallback",
    "dejavu/DejaVuSans-HarbourFallback.ttf",
    DEJAVU_METRICS
);

/// The runtime fallback of proportional families after [`FALLBACK`]: the
/// complete DejaVu Sans Book, for symbols (● ✓ ⌃ ⌄) and scripts (Cyrillic,
/// Greek) in UI strings and resource names that the Harbour faces lack.
pub const DEJAVU_SANS: Face = face!(
    "dejavu-sans",
    "DejaVu Sans",
    "dejavu/DejaVuSans.ttf",
    DEJAVU_METRICS
);

/// The runtime fallback of monospace families after [`FALLBACK`]: the
/// complete DejaVu Sans Mono, so fallback glyphs in code and logs keep a
/// monospace advance.
pub const DEJAVU_SANS_MONO: Face = face!(
    "dejavu-sans-mono",
    "DejaVu Sans Mono",
    "dejavu/DejaVuSansMono.ttf",
    DEJAVU_METRICS
);

/// Every fallback face, in no particular order.
const FALLBACK_FACES: [&Face; 3] = [&FALLBACK, &DEJAVU_SANS, &DEJAVU_SANS_MONO];

/// egui's bundled emoji fonts, kept after [`FALLBACK`] for arbitrary text
/// (log lines, resource names) and the few symbol glyphs the UI still uses.
const EMOJI_FALLBACKS: [&str; 2] = ["NotoEmoji-Regular", "emoji-icon-font"];

/// Every primary face, in registration order.
pub fn faces() -> impl Iterator<Item = &'static Face> {
    [
        &FIGTREE_REGULAR,
        &FIGTREE_MEDIUM,
        &FIGTREE_SEMIBOLD,
        &FIGTREE_BOLD,
        &FIGTREE_TABULAR_REGULAR,
        &FIGTREE_TABULAR_MEDIUM,
        &FIGTREE_TABULAR_SEMIBOLD,
        &FIGTREE_TABULAR_BOLD,
        &MONO_REGULAR,
        &MONO_MEDIUM,
        &MONO_SEMIBOLD,
        &CAPTION_MEDIUM,
        &CAPTION_SEMIBOLD,
    ]
    .into_iter()
    .chain(BRICOLAGE_BOLD.iter())
}

/// The monospaced primary faces; their families fall back to
/// [`DEJAVU_SANS_MONO`] instead of [`DEJAVU_SANS`].
const MONOSPACE_FACES: [&Face; 3] = [&MONO_REGULAR, &MONO_MEDIUM, &MONO_SEMIBOLD];

/// The fallback chain after `primary`: the design subset, the complete
/// DejaVu face of the same kind, then egui's emoji fonts.
fn fallbacks(primary: &Face, fonts: &FontDefinitions) -> Vec<String> {
    let monospace = MONOSPACE_FACES.iter().any(|face| face.key == primary.key);
    let dejavu = if monospace {
        &DEJAVU_SANS_MONO
    } else {
        &DEJAVU_SANS
    };
    [FALLBACK.key, dejavu.key]
        .into_iter()
        .chain(
            EMOJI_FALLBACKS
                .into_iter()
                .filter(|key| fonts.font_data.contains_key(*key)),
        )
        .map(str::to_owned)
        .collect()
}

/// The font definitions of the Harbour font set.
///
/// `Proportional` is Figtree Regular and `Monospace` is IBM Plex Mono
/// Regular; egui's own Ubuntu and Hack faces are dropped.
pub fn definitions() -> FontDefinitions {
    let defaults = FontDefinitions::default();
    let mut fonts = FontDefinitions::empty();
    for key in EMOJI_FALLBACKS {
        if let Some(data) = defaults.font_data.get(key) {
            fonts.font_data.insert(key.to_owned(), Arc::clone(data));
        }
    }
    for face in faces().chain(FALLBACK_FACES) {
        fonts.font_data.insert(
            face.key.to_owned(),
            Arc::new(FontData::from_static(face.bytes)),
        );
    }
    let stack = |primary: &Face, fonts: &FontDefinitions| -> Vec<String> {
        std::iter::once(primary.key.to_owned())
            .chain(fallbacks(primary, fonts))
            .collect()
    };
    for face in faces() {
        let family = stack(face, &fonts);
        fonts
            .families
            .insert(FontFamily::Name(face.family.into()), family);
    }
    let proportional = stack(&FIGTREE_REGULAR, &fonts);
    fonts
        .families
        .insert(FontFamily::Proportional, proportional);
    let monospace = stack(&MONO_REGULAR, &fonts);
    fonts.families.insert(FontFamily::Monospace, monospace);
    fonts
}

/// Register the Harbour font set with `ctx`.
///
/// Like every `set_fonts`, this takes effect at the start of the next pass;
/// call it before the first frame.
pub fn install(ctx: &Context) {
    ctx.set_fonts(definitions());
}

/// Whether the Harbour families are active in `ctx` this pass.
///
/// egui panics when laying out text in an unregistered family. Fonts
/// installed with [`install`] only become active at the start of the next
/// pass, so the very first pass of a context (for example the bootstrap
/// frame of a test harness, which runs before its setup) has none of them.
pub fn are_active(ctx: &Context) -> bool {
    static PROBE: LazyLock<FontFamily> =
        LazyLock::new(|| FontFamily::Name(family::FIGTREE_REGULAR.into()));
    ctx.fonts(|fonts| fonts.definitions().families.contains_key(&PROBE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_u16(bytes: &[u8], offset: usize) -> u16 {
        u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
    }

    fn read_i16(bytes: &[u8], offset: usize) -> i16 {
        read_u16(bytes, offset) as i16
    }

    fn table(bytes: &[u8], tag: &[u8; 4]) -> usize {
        let count = read_u16(bytes, 4) as usize;
        (0..count)
            .map(|index| 12 + index * 16)
            .find(|record| &bytes[*record..*record + 4] == tag)
            .map(|record| {
                u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap()) as usize
            })
            .unwrap_or_else(|| panic!("missing {} table", String::from_utf8_lossy(tag)))
    }

    /// The metrics egui (skrifa) and Chromium read: `OS/2` typo metrics when
    /// `USE_TYPO_METRICS` is set, `hhea` otherwise.
    fn file_metrics(bytes: &[u8]) -> (FaceMetrics, f32) {
        let units_per_em = f32::from(read_u16(bytes, table(bytes, b"head") + 18));
        let os2 = table(bytes, b"OS/2");
        let use_typo = read_u16(bytes, os2 + 62) & (1 << 7) != 0;
        let (ascent, descent, gap) = if use_typo {
            (
                read_i16(bytes, os2 + 68),
                read_i16(bytes, os2 + 70),
                read_i16(bytes, os2 + 72),
            )
        } else {
            let hhea = table(bytes, b"hhea");
            (
                read_i16(bytes, hhea + 4),
                read_i16(bytes, hhea + 6),
                read_i16(bytes, hhea + 8),
            )
        };
        (
            FaceMetrics {
                ascent: f32::from(ascent) / units_per_em,
                descent: -f32::from(descent) / units_per_em,
            },
            f32::from(gap) / units_per_em,
        )
    }

    #[test]
    fn declared_metrics_match_the_font_files() {
        for face in faces().chain(FALLBACK_FACES) {
            let (metrics, gap) = file_metrics(face.bytes);
            assert_eq!(metrics, face.metrics, "{}", face.key);
            assert_eq!(gap, 0.0, "{} has a line gap", face.key);
        }
    }

    #[test]
    fn every_family_ends_with_the_shared_fallbacks() {
        let fonts = definitions();
        for face in faces() {
            let stack = &fonts.families[&FontFamily::Name(face.family.into())];
            let dejavu = if face.family.starts_with("IBM Plex Mono") {
                DEJAVU_SANS_MONO.key
            } else {
                DEJAVU_SANS.key
            };
            assert_eq!(
                stack,
                &[
                    face.key,
                    FALLBACK.key,
                    dejavu,
                    EMOJI_FALLBACKS[0],
                    EMOJI_FALLBACKS[1]
                ],
                "{}",
                face.family
            );
        }
        assert_eq!(
            fonts.families[&FontFamily::Monospace][2],
            DEJAVU_SANS_MONO.key
        );
        assert_eq!(
            fonts.families[&FontFamily::Proportional][2],
            DEJAVU_SANS.key
        );
        assert_eq!(
            fonts.families[&FontFamily::Proportional][0],
            FIGTREE_REGULAR.key
        );
        assert_eq!(fonts.families[&FontFamily::Monospace][0], MONO_REGULAR.key);
        assert!(!fonts.font_data.contains_key("Ubuntu-Light"));
        assert!(!fonts.font_data.contains_key("Hack"));
    }

    #[test]
    fn display_sizes_map_onto_their_optical_size_instance() {
        assert_eq!(bricolage(28.0).key, "bricolage-bold-opsz28");
        assert_eq!(bricolage(13.0).key, "bricolage-bold-opsz13");
        assert!(std::panic::catch_unwind(|| bricolage(24.0)).is_err());
    }

    #[test]
    fn families_become_active_on_the_next_pass() {
        let ctx = Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| assert!(!are_active(ui.ctx())));
        output.textures_delta.clear();
        install(&ctx);
        let mut output = ctx.run_ui(Default::default(), |ui| assert!(are_active(ui.ctx())));
        output.textures_delta.clear();
    }
}
