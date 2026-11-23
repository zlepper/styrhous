//! Parity between the Rust theme tables and the design's CSS tokens.
//!
//! `harbour-dark.css` is a verbatim copy of the design board's `:root` block
//! and its scoped custom properties. These tests fail when either side gains,
//! loses or changes a token.

use std::collections::{BTreeMap, BTreeSet};

use egui::Color32;

use super::{Override, Role, Theme, ThemeScope};
use crate::design::elevation::Elevation;
use crate::design::palette::PaletteKey;

const FIXTURE: &str = include_str!("../harbour-dark.css");

/// A CSS rule reduced to its custom property declarations.
struct Rule {
    selector: String,
    properties: Vec<(String, String)>,
}

fn strip_comments(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        let end = rest[start..].find("*/").expect("unterminated comment") + start + 2;
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn rules() -> Vec<Rule> {
    strip_comments(FIXTURE)
        .split('}')
        .filter_map(|chunk| {
            let (selector, body) = chunk.split_once('{')?;
            let properties = body
                .split(';')
                .filter_map(|declaration| {
                    let (name, value) = declaration.trim().strip_prefix("--")?.split_once(':')?;
                    Some((name.trim().to_owned(), value.trim().to_owned()))
                })
                .collect();
            Some(Rule {
                selector: selector.trim().to_owned(),
                properties,
            })
        })
        .collect()
}

fn root_tokens() -> BTreeMap<String, String> {
    let rules = rules();
    let root = rules
        .iter()
        .find(|rule| rule.selector == ":root")
        .expect("fixture has a :root block");
    let tokens: BTreeMap<_, _> = root.properties.iter().cloned().collect();
    assert_eq!(tokens.len(), root.properties.len(), "duplicate CSS token");
    tokens
}

/// `var(--name)` → `name`.
fn reference(value: &str) -> Option<&str> {
    value.strip_prefix("var(--")?.strip_suffix(')')
}

fn literal_color(value: &str) -> Color32 {
    if value.starts_with('#') {
        return Color32::from_hex(value).unwrap_or_else(|_| panic!("bad hex {value}"));
    }
    let channels = value
        .strip_prefix("rgba(")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_else(|| panic!("unsupported colour literal {value}"));
    let parts: Vec<&str> = channels.split(',').map(str::trim).collect();
    let [r, g, b, a] = parts[..] else {
        panic!("rgba needs four components: {value}");
    };
    let alpha: f32 = a.parse().unwrap();
    Color32::from_rgba_unmultiplied(
        r.parse().unwrap(),
        g.parse().unwrap(),
        b.parse().unwrap(),
        (alpha * 255.0).round() as u8,
    )
}

/// Resolve a token through `var()` chains to its colour.
fn resolve(tokens: &BTreeMap<String, String>, name: &str) -> Color32 {
    let value = &tokens[name];
    match reference(value) {
        Some(target) => resolve(tokens, target),
        None => literal_color(value),
    }
}

/// One `box-shadow` layer: `x y blur [spread] var(--colour)`.
#[derive(Debug, PartialEq)]
struct ShadowLayer {
    lengths: Vec<i32>,
    color: String,
}

fn shadow_layers(value: &str) -> Vec<ShadowLayer> {
    value
        .split(',')
        .map(|layer| {
            let mut parts: Vec<&str> = layer.split_whitespace().collect();
            let color = reference(parts.pop().expect("empty shadow layer"))
                .expect("shadow colours reference the palette")
                .to_owned();
            let lengths = parts
                .iter()
                .map(|part| part.trim_end_matches("px").parse().unwrap())
                .collect();
            ShadowLayer { lengths, color }
        })
        .collect()
}

#[test]
fn every_css_token_exists_in_rust_with_the_same_definition() {
    for (name, value) in root_tokens() {
        if let Some(entry) = PaletteKey::from_key(&name) {
            assert_eq!(entry.harbour(), literal_color(&value), "palette `{name}`");
        } else if let Some(role) = Role::from_key(&name) {
            let target = reference(&value).unwrap_or_else(|| panic!("role `{name}` = {value}"));
            assert_eq!(
                Theme::default().definition().role(role).key(),
                target,
                "role `{name}`"
            );
        } else if let Some(component) = Override::from_key(&name) {
            let target = reference(&value).unwrap_or_else(|| panic!("override `{name}` = {value}"));
            assert_eq!(
                Theme::default().definition().component(component).key(),
                target,
                "override `{name}`"
            );
        } else if let Some(elevation) = Elevation::ALL.iter().find(|e| e.key() == name) {
            let spec = elevation.spec();
            let mut expected = vec![ShadowLayer {
                lengths: vec![0, spec.offset_y.into(), spec.blur.into()],
                color: spec.color.key().to_owned(),
            }];
            if let Some(ring) = spec.ring {
                expected.push(ShadowLayer {
                    lengths: vec![0, 0, 0, 1],
                    color: ring.key().to_owned(),
                });
            }
            assert_eq!(shadow_layers(&value), expected, "shadow `{name}`");
        } else {
            panic!("CSS token `--{name}` has no Rust counterpart");
        }
    }
}

#[test]
fn every_rust_key_exists_in_css() {
    let tokens = root_tokens();
    let rust_keys = PaletteKey::ALL
        .iter()
        .map(|entry| entry.key())
        .chain(Role::ALL.iter().map(|role| role.key()))
        .chain(Override::ALL.iter().map(|component| component.key()))
        .chain(Elevation::ALL.iter().map(|elevation| elevation.key()));
    for key in rust_keys {
        assert!(
            tokens.contains_key(key),
            "Rust key `{key}` is missing from the CSS"
        );
    }
}

#[test]
fn layers_only_reference_the_layer_below() {
    for (name, value) in root_tokens() {
        let Some(target) = reference(&value) else {
            continue;
        };
        if Role::from_key(&name).is_some() {
            assert!(
                PaletteKey::from_key(target).is_some(),
                "role `{name}` → `{target}`"
            );
        } else if Override::from_key(&name).is_some() {
            assert!(
                Role::from_key(target).is_some(),
                "override `{name}` → `{target}`"
            );
        } else {
            panic!("`{name}` references `{target}` but is neither a role nor an override");
        }
    }
}

#[test]
fn resolved_theme_matches_css_cascade() {
    let tokens = root_tokens();
    let theme = Theme::default();
    for &role in Role::ALL {
        assert_eq!(
            theme.role(role),
            resolve(&tokens, role.key()),
            "{}",
            role.key()
        );
    }
    for &component in Override::ALL {
        assert_eq!(
            theme.components.get(component),
            resolve(&tokens, component.key()),
            "{}",
            component.key()
        );
    }
}

/// The scope a single CSS class selector stands for.
fn scope_for_class(class: &str) -> ThemeScope {
    match class {
        ".card" => ThemeScope::Card,
        ".blade" => ThemeScope::Blade,
        ".dlg" => ThemeScope::Dialog,
        ".pop" => ThemeScope::Popover,
        ".side" => ThemeScope::Sidebar,
        ".palette" => ThemeScope::CommandPalette,
        ".ftool" => ThemeScope::FloatingToolbar,
        other => panic!("CSS scope `{other}` has no ThemeScope"),
    }
}

#[test]
fn scoped_css_rules_match_scope_repointing() {
    let mut covered = BTreeSet::new();
    for rule in rules().into_iter().filter(|rule| rule.selector != ":root") {
        let expected: BTreeSet<(Override, Role)> = rule
            .properties
            .iter()
            .map(|(name, value)| {
                let component = Override::from_key(name).unwrap_or_else(|| {
                    panic!("`{}` re-points non-override `{name}`", rule.selector)
                });
                let role = reference(value)
                    .and_then(Role::from_key)
                    .unwrap_or_else(|| {
                        panic!("`{}` points `{name}` at non-role {value}", rule.selector)
                    });
                (component, role)
            })
            .collect();

        // `.a .b, .c .b` lists the ancestors under which `.b` changes mapping.
        let compounds: Vec<Vec<&str>> = rule
            .selector
            .split(',')
            .map(|compound| compound.split_whitespace().collect())
            .collect();
        let target = scope_for_class(compounds[0].last().unwrap());
        let ancestors: Vec<ThemeScope> = compounds
            .iter()
            .filter_map(|compound| {
                assert_eq!(scope_for_class(compound.last().unwrap()), target);
                compound
                    .first()
                    .filter(|_| compound.len() == 2)
                    .map(|a| scope_for_class(a))
            })
            .collect();
        let in_overlay = !ancestors.is_empty();
        for scope in ThemeScope::ALL {
            assert_eq!(
                ancestors.contains(&scope),
                in_overlay && scope.is_overlay(),
                "overlay ancestry of {scope:?} for `{}`",
                rule.selector
            );
        }

        let actual: BTreeSet<_> = target.repoints(in_overlay).iter().copied().collect();
        assert_eq!(actual, expected, "re-pointing for `{}`", rule.selector);
        covered.insert(format!("{target:?}/{in_overlay}"));
    }
    for scope in ThemeScope::ALL {
        assert!(
            covered.contains(&format!("{scope:?}/false")),
            "{scope:?} has no CSS rule"
        );
    }
}
