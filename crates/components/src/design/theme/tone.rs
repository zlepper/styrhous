//! Tones: the status colour families (`ok`, `warn`, `bad`, `info`, `acc`
//! and neutral in the design's CSS), resolved from roles in one place.

use egui::Color32;

use super::Theme;

/// A colour family for status dots, pills, tiles, callouts and banners.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tone {
    /// No status: `status-neutral` dots, `well-bg` pills.
    #[default]
    Neutral,
    /// The accent (`.acc`).
    Accent,
    /// Informational (`.info`).
    Info,
    /// Healthy or completed (`.ok`).
    Success,
    /// Degraded or pending (`.warn`).
    Warning,
    /// Failed or destructive (`.bad`).
    Danger,
}

/// The colours of one [`Tone`] in a theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneColors {
    /// Solid fill: status dots, solid tiles, chart marks (`.dot.ok`, `.tile.ok`).
    pub fill: Color32,
    /// Quiet background: pills, callouts, banners (`.pill.ok`, `.callout.ok`).
    pub tint: Color32,
    /// Coloured text on a surface (`.st-bad`, `.pill.ok` text).
    pub text: Color32,
    /// Body text on [`Self::tint`] (`.banner.warn`).
    pub text_on_tint: Color32,
    /// Hairline around a tinted area (`.banner.warn` border).
    pub outline: Color32,
}

impl Theme {
    /// The colours of `tone`.
    ///
    /// Roles a tone does not define fall back as follows:
    ///
    /// - **Neutral** has no status roles: `fill` is `status-neutral`, `tint`
    ///   the `well-bg` override (neutral pills and chips), `text` and
    ///   `text_on_tint` are `text-secondary`, `outline` is `divider-strong`.
    /// - **Accent** and **Success** have no `*-text-on-tint`: `text_on_tint`
    ///   is their `*-text`.
    pub fn tone(&self, tone: Tone) -> ToneColors {
        match tone {
            Tone::Neutral => ToneColors {
                fill: self.status.neutral,
                tint: self.components.well_bg,
                text: self.text.secondary,
                text_on_tint: self.text.secondary,
                outline: self.divider.strong,
            },
            Tone::Accent => ToneColors {
                fill: self.accent.fill,
                tint: self.accent.tint,
                text: self.accent.text,
                text_on_tint: self.accent.text,
                outline: self.accent.outline,
            },
            Tone::Info => ToneColors {
                fill: self.info.fill,
                tint: self.info.tint,
                text: self.info.text,
                text_on_tint: self.info.text_on_tint,
                outline: self.info.outline,
            },
            Tone::Success => ToneColors {
                fill: self.success.fill,
                tint: self.success.tint,
                text: self.success.text,
                text_on_tint: self.success.text,
                outline: self.success.outline,
            },
            Tone::Warning => ToneColors {
                fill: self.warning.fill,
                tint: self.warning.tint,
                text: self.warning.text,
                text_on_tint: self.warning.text_on_tint,
                outline: self.warning.outline,
            },
            Tone::Danger => ToneColors {
                fill: self.danger.fill,
                tint: self.danger.tint,
                text: self.danger.text,
                text_on_tint: self.danger.text_on_tint,
                outline: self.danger.outline,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::theme::ThemeScope;

    #[test]
    fn tones_read_their_status_roles() {
        let theme = Theme::default();
        let danger = theme.tone(Tone::Danger);
        assert_eq!(danger.fill, theme.danger.fill);
        assert_eq!(danger.tint, theme.danger.tint);
        assert_eq!(danger.text, theme.danger.text);
        assert_eq!(danger.text_on_tint, theme.danger.text_on_tint);
        assert_eq!(danger.outline, theme.danger.outline);
        assert_eq!(theme.tone(Tone::Info).text_on_tint, theme.info.text_on_tint);
        assert_eq!(theme.tone(Tone::Warning).fill, theme.warning.fill);
    }

    #[test]
    fn missing_roles_fall_back_as_documented() {
        let theme = Theme::default();
        let neutral = theme.tone(Tone::Neutral);
        assert_eq!(neutral.fill, theme.status.neutral);
        assert_eq!(neutral.tint, theme.components.well_bg);
        assert_eq!(neutral.text_on_tint, theme.text.secondary);
        assert_eq!(theme.tone(Tone::Success).text_on_tint, theme.success.text);
        assert_eq!(theme.tone(Tone::Accent).text_on_tint, theme.accent.text);
    }

    #[test]
    fn the_neutral_tint_follows_scope_repointing() {
        let theme = Theme::default();
        let palette = theme.scoped(ThemeScope::CommandPalette);
        assert_eq!(palette.tone(Tone::Neutral).tint, theme.surface.panel);
    }
}
