//! User theme files.
//!
//! A theme file is TOML listing only its changes on top of exactly one
//! built-in theme. There are no chains: `extends` names a built-in theme.
//!
//! ```toml
//! name = "Harbour Dark · high contrast"
//! extends = "harbour-dark"     # built-in themes only; defaults to harbour-dark
//!
//! [palette]                    # palette entries = hex colours
//! sand-300 = "#B3A89B"         # #RGB, #RGBA, #RRGGBB or #RRGGBBAA
//!
//! [roles]                      # semantic roles = palette names
//! text-muted = "sand-200"
//!
//! [components]                 # component overrides = role names
//! pill-on-bg = "accent-tint"
//!
//! [terminal]                   # code, log, syntax and ANSI roles = palette names
//! ansi-blue = "sky-300"
//! ```
//!
//! Overriding a palette entry changes every role and override that points
//! at it.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;

use super::{BuiltinTheme, Override, Role, ThemeDefinition, ThemeSection};
use crate::design::palette::{self, PaletteKey};

/// The design's example theme file (board C00b), verbatim: Harbour Dark with
/// higher-contrast muted text, a sky focus ring and a lighter accent.
pub const EXAMPLE_THEME_FILE: &str = include_str!("harbour-dark-hc.toml");

/// The parsed, not yet validated contents of a theme file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFile {
    /// Display name of the theme.
    pub name: String,
    /// The built-in theme the file changes. Defaults to Harbour Dark.
    #[serde(default)]
    pub extends: Option<String>,
    /// Palette entry name → hex colour.
    #[serde(default)]
    pub palette: BTreeMap<String, String>,
    /// Role name → palette entry name.
    #[serde(default)]
    pub roles: BTreeMap<String, String>,
    /// Component override name → role name.
    #[serde(default)]
    pub components: BTreeMap<String, String>,
    /// Code, log, syntax or ANSI role name → palette entry name.
    #[serde(default)]
    pub terminal: BTreeMap<String, String>,
}

/// Why a theme file could not be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeError {
    /// The file is not valid TOML or does not have the theme file shape.
    Syntax(String),
    /// The file parsed but contains invalid entries. Every problem is listed.
    Invalid {
        /// The theme's `name`.
        name: String,
        issues: Vec<ThemeIssue>,
    },
}

/// One invalid entry in a theme file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeIssue {
    /// `extends` does not name a built-in theme.
    UnknownBase { name: String },
    /// A key that does not exist anywhere in the theme.
    UnknownKey {
        section: ThemeSection,
        key: String,
        suggestion: Option<&'static str>,
    },
    /// A key that exists, but belongs in another table.
    WrongSection {
        key: String,
        found: ThemeSection,
        expected: ThemeSection,
    },
    /// A palette value that is not a hex colour.
    InvalidColor { key: String, value: String },
    /// A role or override that points at a name that does not exist in the
    /// layer below.
    UnknownReference {
        section: ThemeSection,
        key: String,
        target: String,
        suggestion: Option<&'static str>,
    },
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(message) => write!(f, "theme file is not valid: {message}"),
            Self::Invalid { name, issues } => {
                write!(f, "theme \"{name}\" has {} invalid entries:", issues.len())?;
                for issue in issues {
                    write!(f, "\n  - {issue}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ThemeError {}

impl fmt::Display for ThemeIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownBase { name } => {
                let available: Vec<_> = BuiltinTheme::ALL.iter().map(|theme| theme.key()).collect();
                write!(
                    f,
                    "extends = \"{name}\" is not a built-in theme (available: {})",
                    available.join(", ")
                )
            }
            Self::UnknownKey {
                section,
                key,
                suggestion,
            } => {
                write!(f, "{section} has no key `{key}`")?;
                write_suggestion(f, *suggestion)
            }
            Self::WrongSection {
                key,
                found,
                expected,
            } => write!(f, "`{key}` belongs in {expected}, not {found}"),
            Self::InvalidColor { key, value } => write!(
                f,
                "[palette] `{key}` = \"{value}\" is not a hex colour such as \"#B3A89B\""
            ),
            Self::UnknownReference {
                section,
                key,
                target,
                suggestion,
            } => {
                let expected = match section {
                    ThemeSection::Components => "role",
                    _ => "palette entry",
                };
                write!(
                    f,
                    "{section} `{key}` points at unknown {expected} `{target}`"
                )?;
                write_suggestion(f, *suggestion)
            }
        }
    }
}

fn write_suggestion(f: &mut fmt::Formatter<'_>, suggestion: Option<&str>) -> fmt::Result {
    match suggestion {
        Some(suggestion) => write!(f, "; did you mean `{suggestion}`?"),
        None => Ok(()),
    }
}

impl ThemeFile {
    /// Parse TOML into a theme file without validating names.
    pub fn parse(source: &str) -> Result<Self, ThemeError> {
        toml::from_str(source).map_err(|error| ThemeError::Syntax(error.to_string()))
    }

    /// Validate the file and apply it on top of its built-in theme.
    ///
    /// All invalid entries are reported together.
    pub fn definition(&self) -> Result<ThemeDefinition, ThemeError> {
        let mut issues = Vec::new();
        let base = match self.extends.as_deref() {
            None => BuiltinTheme::default(),
            Some(name) => BuiltinTheme::from_key(name).unwrap_or_else(|| {
                issues.push(ThemeIssue::UnknownBase {
                    name: name.to_owned(),
                });
                BuiltinTheme::default()
            }),
        };

        let mut definition = base.definition();
        definition.set_name(self.name.clone());

        for (key, value) in &self.palette {
            let Some(entry) = lookup_key(
                ThemeSection::Palette,
                key,
                &mut issues,
                PaletteKey::from_key,
            ) else {
                continue;
            };
            match palette::parse_hex(value) {
                Some(color) => definition.set_palette(entry, color),
                None => issues.push(ThemeIssue::InvalidColor {
                    key: key.clone(),
                    value: value.clone(),
                }),
            }
        }

        for (section, table) in [
            (ThemeSection::Roles, &self.roles),
            (ThemeSection::Terminal, &self.terminal),
        ] {
            for (key, target) in table {
                let role = lookup_key(section, key, &mut issues, |key| {
                    Role::from_key(key).filter(|role| role.section() == section)
                });
                let entry =
                    lookup_reference(section, key, target, &mut issues, PaletteKey::from_key);
                if let (Some(role), Some(entry)) = (role, entry) {
                    definition.set_role(role, entry);
                }
            }
        }

        for (key, target) in &self.components {
            let component = lookup_key(
                ThemeSection::Components,
                key,
                &mut issues,
                Override::from_key,
            );
            let role = lookup_reference(
                ThemeSection::Components,
                key,
                target,
                &mut issues,
                Role::from_key,
            );
            if let (Some(component), Some(role)) = (component, role) {
                definition.set_component(component, role);
            }
        }

        if issues.is_empty() {
            Ok(definition)
        } else {
            Err(ThemeError::Invalid {
                name: self.name.clone(),
                issues,
            })
        }
    }
}

/// Resolve a key of `section`, recording an issue when it is unknown or
/// belongs to another table.
fn lookup_key<T>(
    section: ThemeSection,
    key: &str,
    issues: &mut Vec<ThemeIssue>,
    find: impl Fn(&str) -> Option<T>,
) -> Option<T> {
    if let Some(found) = find(key) {
        return Some(found);
    }
    issues.push(match section_of(key) {
        Some(expected) => ThemeIssue::WrongSection {
            key: key.to_owned(),
            found: section,
            expected,
        },
        None => ThemeIssue::UnknownKey {
            section,
            key: key.to_owned(),
            suggestion: closest(key, keys_in(section)),
        },
    });
    None
}

/// Resolve the target of a role (palette entry) or override (role),
/// recording an issue when it does not exist.
fn lookup_reference<T>(
    section: ThemeSection,
    key: &str,
    target: &str,
    issues: &mut Vec<ThemeIssue>,
    find: impl Fn(&str) -> Option<T>,
) -> Option<T> {
    let found = find(target);
    if found.is_none() {
        let candidates: Box<dyn Iterator<Item = &'static str>> = match section {
            ThemeSection::Components => Box::new(Role::ALL.iter().map(|role| role.key())),
            _ => Box::new(PaletteKey::ALL.iter().map(|entry| entry.key())),
        };
        issues.push(ThemeIssue::UnknownReference {
            section,
            key: key.to_owned(),
            target: target.to_owned(),
            suggestion: closest(target, candidates),
        });
    }
    found
}

/// The table a key belongs in, if it exists in any layer.
fn section_of(key: &str) -> Option<ThemeSection> {
    if PaletteKey::from_key(key).is_some() {
        Some(ThemeSection::Palette)
    } else if let Some(role) = Role::from_key(key) {
        Some(role.section())
    } else if Override::from_key(key).is_some() {
        Some(ThemeSection::Components)
    } else {
        None
    }
}

fn keys_in(section: ThemeSection) -> Box<dyn Iterator<Item = &'static str>> {
    match section {
        ThemeSection::Palette => Box::new(PaletteKey::ALL.iter().map(|entry| entry.key())),
        ThemeSection::Components => Box::new(Override::ALL.iter().map(|component| component.key())),
        ThemeSection::Roles | ThemeSection::Terminal => Box::new(
            Role::ALL
                .iter()
                .filter(move |role| role.section() == section)
                .map(|role| role.key()),
        ),
    }
}

/// The candidate closest to `input` by edit distance, if it is close enough
/// to plausibly be a typo. Candidates in the same group (the part before the
/// first `-`, such as `pill` or `text`) win over closer keys of other groups,
/// so `pill-bg` suggests `pill-on-bg` rather than `well-bg`.
fn closest(input: &str, candidates: impl Iterator<Item = &'static str>) -> Option<&'static str> {
    let limit = (input.chars().count() / 3).max(3);
    let group = |key: &'static str| key.split('-').next().unwrap_or(key);
    let input_group = input.split('-').next().unwrap_or(input);
    candidates
        .map(|candidate| (edit_distance(input, candidate), candidate))
        .filter(|(distance, _)| *distance <= limit)
        .min_by_key(|(distance, candidate)| (group(candidate) != input_group, *distance))
        .map(|(_, candidate)| candidate)
}

/// Levenshtein distance between two strings.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, a_char) in a.chars().enumerate() {
        let mut current = Vec::with_capacity(b.len() + 1);
        current.push(i + 1);
        for (j, b_char) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(a_char != *b_char);
            current.push(substitution.min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::palette;
    use crate::design::theme::Theme;

    use egui::Color32;

    const HIGH_CONTRAST: &str = EXAMPLE_THEME_FILE;

    fn issues(source: &str) -> Vec<ThemeIssue> {
        match Theme::from_file_str(source) {
            Err(ThemeError::Invalid { issues, .. }) => issues,
            other => panic!("expected validation issues, got {other:?}"),
        }
    }

    #[test]
    fn design_example_resolves() {
        let theme = Theme::from_file_str(HIGH_CONTRAST).unwrap();
        let lighter = Color32::from_hex("#B3A89B").unwrap();

        assert_eq!(theme.name(), "Harbour Dark · high contrast");
        assert_eq!(theme.focus.ring, palette::SKY_300);
        assert_eq!(theme.accent.fill, palette::CORAL_300);
        assert_eq!(
            theme.accent.text,
            palette::CORAL_400,
            "only accent-fill moves"
        );
        // The role override wins over the palette entry it used to point at...
        assert_eq!(theme.text.muted, palette::SAND_200);
        // ...while every other consumer of sand-300 picks up the new value.
        assert_eq!(theme.control.border_hover, lighter);
        assert_eq!(theme.nav.text_muted, lighter);
        assert_eq!(theme.syntax.punctuation, lighter);
        assert_eq!(theme.code.debug, lighter);

        assert_eq!(theme.components.pill_on_bg, theme.accent.tint);
        assert_eq!(
            theme.components.pill_on_bg_hover,
            theme.inverse.surface_hover
        );
        assert_eq!(theme.ansi.blue, palette::SKY_300);
        assert_eq!(theme.ansi.bright_blue, palette::SKY_300);
    }

    #[test]
    fn palette_only_override_propagates_to_every_consumer() {
        let theme = Theme::from_file_str(
            r##"
            name = "Lighter muted"
            extends = "harbour-dark"
            [palette]
            sand-300 = "#B3A89B"
            "##,
        )
        .unwrap();
        let lighter = Color32::from_hex("#B3A89B").unwrap();
        assert_eq!(theme.text.muted, lighter);
        assert_eq!(theme.control.border_hover, lighter);
        assert_eq!(theme.text.secondary, palette::SAND_200);
    }

    #[test]
    fn translucent_palette_override_reaches_shadows() {
        let theme = Theme::from_file_str(
            r##"
            name = "Deep shadows"
            [palette]
            black-a55 = "#000000CC"
            "##,
        )
        .unwrap();
        let deeper = Color32::from_rgba_unmultiplied(0, 0, 0, 0xCC);
        assert_eq!(theme.shadow.raised.shadow.color, deeper);
        assert_eq!(theme.shadow.drag.shadow.color, deeper);
        assert_eq!(theme.shadow.tooltip.shadow.color, palette::BLACK_A45);
        assert_eq!(theme.definition().base(), BuiltinTheme::HarbourDark);
    }

    #[test]
    fn terminal_only_override_leaves_ui_roles_alone() {
        let theme = Theme::from_file_str(
            r#"
            name = "Solar terminal"
            extends = "harbour-dark"
            [terminal]
            ansi-red = "coral-500"
            syntax-key = "sky-400"
            log-row-error = "red-850"
            "#,
        )
        .unwrap();
        let builtin = Theme::default();
        assert_eq!(theme.ansi.red, palette::CORAL_500);
        assert_eq!(theme.syntax.key, palette::SKY_400);
        assert_eq!(theme.log.row_error, palette::RED_850);
        assert_eq!(theme.danger.fill, builtin.danger.fill);
        assert_eq!(theme.accent.fill, builtin.accent.fill);
    }

    #[test]
    fn component_override_points_at_a_role() {
        let theme = Theme::from_file_str(
            r#"
            name = "Sunken rows"
            extends = "harbour-dark"
            [components]
            row-bg = "surface-sunken"
            chart-cpu = "info-fill"
            "#,
        )
        .unwrap();
        assert_eq!(theme.components.row_bg, theme.surface.sunken);
        assert_eq!(theme.components.chart_cpu, theme.info.fill);
    }

    #[test]
    fn unknown_keys_are_reported_with_suggestions() {
        let found = issues(
            r#"
            name = "Typos"
            [roles]
            text-mutted = "sand-200"
            [components]
            pill-bg = "accent-tint"
            completely-made-up = "accent-tint"
            "#,
        );
        assert_eq!(
            found,
            vec![
                ThemeIssue::UnknownKey {
                    section: ThemeSection::Roles,
                    key: "text-mutted".into(),
                    suggestion: Some("text-muted"),
                },
                ThemeIssue::UnknownKey {
                    section: ThemeSection::Components,
                    key: "completely-made-up".into(),
                    suggestion: None,
                },
                ThemeIssue::UnknownKey {
                    section: ThemeSection::Components,
                    key: "pill-bg".into(),
                    suggestion: Some("pill-on-bg"),
                },
            ]
        );
    }

    #[test]
    fn keys_in_the_wrong_table_name_the_right_one() {
        let found = issues(
            r#"
            name = "Misplaced"
            [roles]
            ansi-blue = "sky-300"
            [terminal]
            text-muted = "sand-200"
            "#,
        );
        assert_eq!(
            found,
            vec![
                ThemeIssue::WrongSection {
                    key: "ansi-blue".into(),
                    found: ThemeSection::Roles,
                    expected: ThemeSection::Terminal,
                },
                ThemeIssue::WrongSection {
                    key: "text-muted".into(),
                    found: ThemeSection::Terminal,
                    expected: ThemeSection::Roles,
                },
            ]
        );
    }

    #[test]
    fn bad_hex_values_are_rejected() {
        let found = issues(
            r##"
            name = "Bad hex"
            [palette]
            sand-300 = "B3A89B"
            sand-400 = "#B3A8"
            sand-500 = "#GGGGGG"
            "##,
        );
        // #B3A8 is valid 4-digit hex (#RGBA); the other two are not.
        assert_eq!(
            found,
            vec![
                ThemeIssue::InvalidColor {
                    key: "sand-300".into(),
                    value: "B3A89B".into(),
                },
                ThemeIssue::InvalidColor {
                    key: "sand-500".into(),
                    value: "#GGGGGG".into(),
                },
            ]
        );
    }

    #[test]
    fn references_must_exist_in_the_layer_below() {
        let found = issues(
            r##"
            name = "Bad references"
            [roles]
            text-muted = "#B3A89B"
            accent-fill = "coral-450"
            [components]
            row-bg = "sand-800"
            "##,
        );
        assert_eq!(
            found,
            vec![
                ThemeIssue::UnknownReference {
                    section: ThemeSection::Roles,
                    key: "accent-fill".into(),
                    target: "coral-450".into(),
                    suggestion: Some("coral-400"),
                },
                ThemeIssue::UnknownReference {
                    section: ThemeSection::Roles,
                    key: "text-muted".into(),
                    target: "#B3A89B".into(),
                    suggestion: None,
                },
                ThemeIssue::UnknownReference {
                    section: ThemeSection::Components,
                    key: "row-bg".into(),
                    target: "sand-800".into(),
                    suggestion: None,
                },
            ]
        );
        let message = Theme::from_file_str(
            r#"
            name = "Bad references"
            [components]
            row-bg = "sand-800"
            "#,
        )
        .unwrap_err()
        .to_string();
        assert!(
            message.contains("[components] `row-bg` points at unknown role `sand-800`"),
            "{message}"
        );
    }

    #[test]
    fn extends_only_accepts_builtin_themes() {
        let found = issues(
            r#"
            name = "Chained"
            extends = "my-other-theme"
            "#,
        );
        assert_eq!(
            found,
            vec![ThemeIssue::UnknownBase {
                name: "my-other-theme".into()
            }]
        );
        assert!(found[0].to_string().contains("available: harbour-dark"));
    }

    #[test]
    fn malformed_files_are_syntax_errors() {
        assert!(matches!(
            Theme::from_file_str("name = \"Missing quote"),
            Err(ThemeError::Syntax(_))
        ));
        assert!(matches!(
            Theme::from_file_str("name = \"Extra table\"\n[colours]\nsand-300 = \"#000\""),
            Err(ThemeError::Syntax(message)) if message.contains("colours")
        ));
        assert!(matches!(
            Theme::from_file_str("extends = \"harbour-dark\""),
            Err(ThemeError::Syntax(message)) if message.contains("name")
        ));
    }

    /// The text of the "Theme file" code block on the C00b board.
    fn design_board_example() -> String {
        let board = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/design-oracles/harbour/src/C00b.html");
        let html = std::fs::read_to_string(&board)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", board.display()));
        let marker = "Theme file · overrides any layer</div><div class=\"codeblk\">";
        let start = html.find(marker).expect("C00b has the theme file example") + marker.len();
        let end = start + html[start..].find("</div>").expect("the code block ends");
        html[start..end]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&")
    }

    #[test]
    fn the_example_fixture_is_the_design_example_verbatim() {
        assert_eq!(
            EXAMPLE_THEME_FILE.trim_end(),
            design_board_example().trim_end()
        );
    }
}
