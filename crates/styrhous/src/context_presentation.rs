//! Presentation metadata belongs to a context, independently of its connection identity.

use serde::{Deserialize, Serialize};

pub(crate) const CONTEXT_PRESENTATION_EXTENSION: &str = "io.github.zlepper.styrhous";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ContextPresentation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
}

impl ContextPresentation {
    pub fn display_name<'a>(&'a self, context_name: &'a str) -> &'a str {
        self.display_name.as_deref().unwrap_or(context_name)
    }

    pub fn badge(&self, context_name: &str) -> String {
        self.badge
            .clone()
            .unwrap_or_else(|| automatic_badge(self.display_name(context_name)))
    }

    pub fn normalized(&self) -> Result<Self, String> {
        let trimmed = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        let display_name = trimmed(&self.display_name);
        let badge = trimmed(&self.badge);
        if display_name
            .as_ref()
            .is_some_and(|name| name.chars().any(char::is_control))
        {
            return Err("Display names must fit on one line.".into());
        }
        if badge.as_ref().is_some_and(|badge| {
            badge.chars().count() > 2 || badge.chars().any(|c| c.is_whitespace() || c.is_control())
        }) {
            return Err("Use one or two characters without spaces for the badge.".into());
        }
        Ok(Self {
            display_name,
            badge,
        })
    }

    /// Validate fields separately so one malformed preference cannot hide another.
    pub(crate) fn from_extension(value: &serde_json::Value) -> Self {
        let display_name = Self {
            display_name: value
                .get("display-name")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            ..Self::default()
        }
        .normalized()
        .unwrap_or_default()
        .display_name;
        let badge = Self {
            badge: value
                .get("badge")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            ..Self::default()
        }
        .normalized()
        .unwrap_or_default()
        .badge;
        Self {
            display_name,
            badge,
        }
    }
}

pub(crate) fn automatic_badge(name: &str) -> String {
    let mut words = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty());
    let Some(first) = words.next() else {
        return "?".into();
    };
    let characters: String = match words.next() {
        Some(second) => first
            .chars()
            .take(1)
            .chain(second.chars().take(1))
            .collect(),
        None => first.chars().take(2).collect(),
    };
    characters
        .chars()
        .flat_map(char::to_uppercase)
        .take(2)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_badges_distinguish_context_words_and_support_single_names() {
        for (name, expected) in [
            ("cloudstorage-production-eu", "CP"),
            ("cloudstorage-staging-eu", "CS"),
            ("cloudstorage_(dev) / eu", "CD"),
            ("production", "PR"),
            ("k", "K"),
            ("---", "?"),
            ("åland øresund", "ÅØ"),
            ("ß-east", "SS"),
        ] {
            assert_eq!(automatic_badge(name), expected, "{name}");
        }
    }

    #[test]
    fn overrides_are_trimmed_and_clearing_restores_automatic_identity() {
        let mut presentation = ContextPresentation {
            display_name: Some(" Cloudstorage Prod ".into()),
            badge: Some(" pR ".into()),
        }
        .normalized()
        .unwrap();
        assert_eq!(presentation.display_name("original"), "Cloudstorage Prod");
        assert_eq!(presentation.badge("original"), "pR");
        presentation.badge = Some(" ".into());
        presentation = presentation.normalized().unwrap();
        assert_eq!(presentation.badge("original"), "CP");
        presentation.display_name = None;
        assert_eq!(presentation.badge("original"), "OR");
        for badge in ["abc", "a b", "a\nb", "a\0"] {
            assert!(
                ContextPresentation {
                    badge: Some(badge.into()),
                    ..Default::default()
                }
                .normalized()
                .is_err()
            );
        }
        assert_eq!(
            ContextPresentation::from_extension(
                &serde_json::json!({"display-name": "Friendly", "badge": "too long"})
            )
            .badge("original"),
            "FR"
        );
    }
}
