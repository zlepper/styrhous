//! Layer 1 of the theme: the Harbour palette.
//!
//! The palette is the only place in the application where colour values are
//! written. Nothing but [semantic roles](super::theme::Role) may reference it;
//! components read roles, never palette entries.
//!
//! Every entry exists twice, generated from one table so the two cannot drift:
//!
//! - a `pub const` [`Color32`] (for example [`SAND_950`]) holding the built-in
//!   Harbour value, and
//! - a [`PaletteKey`] variant (for example [`PaletteKey::Sand950`]) carrying the
//!   entry's theme-file name (`"sand-950"`). Roles reference palette entries by
//!   key, so a theme file that overrides `sand-950` changes every role that
//!   points at it.
//!
//! Translucent entries (`black-a55`, `coral-a16`, …) are used only by shadow,
//! scrim, glow and halo roles. Their alpha is given exactly as in the CSS
//! design tokens and converted the way browsers composite `rgba()`: egui
//! blends premultiplied colours in gamma space, so premultiplying the sRGB
//! channels by the CSS alpha renders identically.

use egui::Color32;

/// An opaque colour from a CSS `#RRGGBB` literal written as `0xRRGGBB`.
const fn hex(rgb: u32) -> Color32 {
    let [_, r, g, b] = rgb.to_be_bytes();
    Color32::from_rgb(r, g, b)
}

/// A translucent colour from a CSS `rgba(r, g, b, alpha)` literal.
///
/// The alpha is rounded to the nearest 8-bit step, which is how browsers
/// store it.
const fn rgba(r: u8, g: u8, b: u8, alpha: f32) -> Color32 {
    let alpha = (alpha * 255.0 + 0.5) as u8;
    Color32::from_rgba_unmultiplied_const(r, g, b, alpha)
}

macro_rules! palette {
    ($( $constant:ident / $variant:ident = $key:literal => $value:expr; )*) => {
        $(
            #[doc = concat!("Palette entry `", $key, "`.")]
            pub const $constant: Color32 = $value;
        )*

        /// The name of a palette entry, as written in theme files.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum PaletteKey {
            $(
                #[doc = concat!("`", $key, "`")]
                $variant,
            )*
        }

        impl PaletteKey {
            /// Every palette entry, in design order.
            pub const ALL: &[Self] = &[$(Self::$variant),*];
            /// The number of palette entries.
            pub const COUNT: usize = Self::ALL.len();

            /// The entry's theme-file name, for example `"sand-950"`.
            pub const fn key(self) -> &'static str {
                match self {
                    $(Self::$variant => $key,)*
                }
            }

            /// The built-in Harbour value of this entry.
            pub const fn harbour(self) -> Color32 {
                match self {
                    $(Self::$variant => $constant,)*
                }
            }
        }
    };
}

palette! {
    SAND_50 / Sand50 = "sand-50" => hex(0xF5EFE6);
    SAND_100 / Sand100 = "sand-100" => hex(0xE7DED3);
    SAND_200 / Sand200 = "sand-200" => hex(0xBDB2A6);
    SAND_300 / Sand300 = "sand-300" => hex(0x9C9184);
    SAND_400 / Sand400 = "sand-400" => hex(0x8A7F74);
    SAND_500 / Sand500 = "sand-500" => hex(0x6B6157);
    SAND_600 / Sand600 = "sand-600" => hex(0x5A4F47);
    SAND_700 / Sand700 = "sand-700" => hex(0x4A4039);
    SAND_750 / Sand750 = "sand-750" => hex(0x3A322C);
    SAND_800 / Sand800 = "sand-800" => hex(0x352E29);
    SAND_825 / Sand825 = "sand-825" => hex(0x2F2925);
    SAND_850 / Sand850 = "sand-850" => hex(0x2C2622);
    SAND_875 / Sand875 = "sand-875" => hex(0x26211E);
    SAND_900 / Sand900 = "sand-900" => hex(0x221D1A);
    SAND_950 / Sand950 = "sand-950" => hex(0x1C1917);
    SAND_975 / Sand975 = "sand-975" => hex(0x171412);
    CORAL_200 / Coral200 = "coral-200" => hex(0xFFB59C);
    CORAL_300 / Coral300 = "coral-300" => hex(0xFF9F80);
    CORAL_400 / Coral400 = "coral-400" => hex(0xFF8A65);
    CORAL_500 / Coral500 = "coral-500" => hex(0xF2764F);
    CORAL_700 / Coral700 = "coral-700" => hex(0x9A5541);
    CORAL_800 / Coral800 = "coral-800" => hex(0x553A30);
    CORAL_850 / Coral850 = "coral-850" => hex(0x45302A);
    CORAL_900 / Coral900 = "coral-900" => hex(0x3A2A23);
    RED_100 / Red100 = "red-100" => hex(0xFFCDC8);
    RED_300 / Red300 = "red-300" => hex(0xFF8F8F);
    RED_350 / Red350 = "red-350" => hex(0xFF8282);
    RED_400 / Red400 = "red-400" => hex(0xFF6B6B);
    RED_500 / Red500 = "red-500" => hex(0xF25555);
    RED_700 / Red700 = "red-700" => hex(0x8C3F3B);
    RED_850 / Red850 = "red-850" => hex(0x472A27);
    RED_900 / Red900 = "red-900" => hex(0x3A2422);
    AMBER_100 / Amber100 = "amber-100" => hex(0xFBE3A6);
    AMBER_400 / Amber400 = "amber-400" => hex(0xF2C14E);
    AMBER_700 / Amber700 = "amber-700" => hex(0x7A6128);
    AMBER_850 / Amber850 = "amber-850" => hex(0x4A3A1C);
    AMBER_900 / Amber900 = "amber-900" => hex(0x3A2F1A);
    MINT_200 / Mint200 = "mint-200" => hex(0xA7F3D0);
    MINT_400 / Mint400 = "mint-400" => hex(0x6EE7B7);
    MINT_700 / Mint700 = "mint-700" => hex(0x2F6B53);
    MINT_900 / Mint900 = "mint-900" => hex(0x1F322A);
    SKY_100 / Sky100 = "sky-100" => hex(0xCDE8FD);
    SKY_300 / Sky300 = "sky-300" => hex(0xA9D8FC);
    SKY_400 / Sky400 = "sky-400" => hex(0x7CC4FA);
    SKY_700 / Sky700 = "sky-700" => hex(0x335A78);
    SKY_900 / Sky900 = "sky-900" => hex(0x1F2E3A);
    VIOLET_400 / Violet400 = "violet-400" => hex(0xB39DFF);
    PINK_200 / Pink200 = "pink-200" => hex(0xF8B4CB);
    PINK_400 / Pink400 = "pink-400" => hex(0xF48FB1);
    TEAL_200 / Teal200 = "teal-200" => hex(0x7FE0D4);
    TEAL_400 / Teal400 = "teal-400" => hex(0x4DD0C1);
    LIME_300 / Lime300 = "lime-300" => hex(0xC3E88D);
    GREEN_300 / Green300 = "green-300" => hex(0x9ADBA0);
    BLACK_A55 / BlackA55 = "black-a55" => rgba(0, 0, 0, 0.55);
    BLACK_A45 / BlackA45 = "black-a45" => rgba(0, 0, 0, 0.45);
    BLACK_A32 / BlackA32 = "black-a32" => rgba(0, 0, 0, 0.32);
    BLACK_A18 / BlackA18 = "black-a18" => rgba(0, 0, 0, 0.18);
    WHITE_A04 / WhiteA04 = "white-a04" => rgba(255, 255, 255, 0.04);
    SAND_950_A60 / Sand950A60 = "sand-950-a60" => rgba(18, 15, 13, 0.6);
    SAND_950_A25 / Sand950A25 = "sand-950-a25" => rgba(28, 25, 23, 0.25);
    SAND_950_A18 / Sand950A18 = "sand-950-a18" => rgba(28, 25, 23, 0.18);
    CORAL_A55 / CoralA55 = "coral-a55" => rgba(255, 138, 101, 0.55);
    CORAL_A16 / CoralA16 = "coral-a16" => rgba(255, 138, 101, 0.16);
    RED_A55 / RedA55 = "red-a55" => rgba(255, 107, 107, 0.55);
    RED_A14 / RedA14 = "red-a14" => rgba(255, 107, 107, 0.14);
    MINT_A55 / MintA55 = "mint-a55" => rgba(110, 231, 183, 0.55);
    AMBER_A55 / AmberA55 = "amber-a55" => rgba(242, 193, 78, 0.55);
    AMBER_A22 / AmberA22 = "amber-a22" => rgba(242, 193, 78, 0.22);
}

impl PaletteKey {
    /// Look up a palette entry by its theme-file name.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|entry| entry.key() == key)
    }

    /// Position of this entry in [`PaletteKey::ALL`], used to index resolved
    /// palette tables.
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Parse a user-written hex colour (`#RGB`, `#RGBA`, `#RRGGBB` or
/// `#RRGGBBAA`), as theme files assign palette entries.
pub fn parse_hex(value: &str) -> Option<Color32> {
    Color32::from_hex(value).ok()
}

/// An opaque colour of its own for each `index` below 256, unlike any
/// Harbour entry. Only for tests that need every role to have a distinct
/// colour (`test_support::distinct_role_theme`).
#[doc(hidden)]
pub const fn distinct(index: usize) -> Color32 {
    assert!(index < 256, "only 256 distinct test colours");
    Color32::from_rgb(index as u8, 255 - index as u8, 0x5A)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_round_trip() {
        for (index, entry) in PaletteKey::ALL.iter().enumerate() {
            assert_eq!(entry.index(), index);
            assert_eq!(PaletteKey::from_key(entry.key()), Some(*entry));
        }
    }

    #[test]
    fn translucent_entries_match_egui_unmultiplied_conversion() {
        assert_eq!(BLACK_A55, Color32::from_rgba_unmultiplied(0, 0, 0, 140));
        assert_eq!(
            CORAL_A16,
            Color32::from_rgba_unmultiplied(255, 138, 101, 41)
        );
        assert_eq!(CORAL_A16.to_srgba_unmultiplied()[3], 41);
    }
}
