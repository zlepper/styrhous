//! Storing the active theme on the egui context.
//!
//! The installed theme lives in the context's temporary data as an
//! `Arc<Theme>`, next to a stack of scoped variants pushed by [`scope`].
//! Reading it is an `Arc` clone.
//!
//! ```
//! use components::design::theme::{self, ThemeExt, ThemeScope};
//! # let ctx = egui::Context::default();
//! # let mut output = ctx.run_ui(Default::default(), |ui| {
//! let fill = ui.tokens().surface.panel;
//! theme::scope(ui, ThemeScope::Blade, |ui| {
//!     // Rows inside a blade sit on surface-raised.
//!     let row = ui.tokens().components.row_bg;
//!     # let _ = (fill, row);
//! });
//! # });
//! # output.textures_delta.clear();
//! ```

use std::sync::{Arc, OnceLock};

use egui::{Context, Id, Ui};

use super::{Theme, ThemeScope};

/// Per-context theme state.
#[derive(Clone, Default)]
struct ThemeState {
    installed: Option<Arc<Theme>>,
    /// Scopes entered with [`scope`], innermost last, with their derived theme.
    stack: Vec<(ThemeScope, Arc<Theme>)>,
    generation: u64,
    /// The `pixels_per_point` egui's visuals were last built for by
    /// [`super::apply_theme`], or `None` while no theme has been applied.
    applied_pixels_per_point: Option<f32>,
}

impl ThemeState {
    fn base(&self) -> Arc<Theme> {
        self.installed.clone().unwrap_or_else(default_theme)
    }

    fn current(&self) -> Arc<Theme> {
        self.stack
            .last()
            .map(|(_, theme)| Arc::clone(theme))
            .unwrap_or_else(|| self.base())
    }
}

fn state_id() -> Id {
    Id::new("components::design::theme::state")
}

fn with_state<R>(ctx: &Context, f: impl FnOnce(&mut ThemeState) -> R) -> R {
    ctx.data_mut(|data| f(data.get_temp_mut_or_default::<ThemeState>(state_id())))
}

/// The built-in default theme, resolved once per process.
///
/// Only for code without an egui context; everything else reads the
/// installed theme through [`current`] or [`ThemeExt::tokens`].
pub fn default_theme() -> Arc<Theme> {
    static DEFAULT: OnceLock<Arc<Theme>> = OnceLock::new();
    Arc::clone(DEFAULT.get_or_init(|| Arc::new(Theme::default())))
}

/// Switch `ctx` to `theme`: apply it to egui's own visuals and text styles
/// ([`super::apply_theme`]), make it the theme every component reads and bump
/// [`generation`] so theme-derived caches are rebuilt.
///
/// This is the one way to change the theme of a running UI. Scopes that are
/// currently entered are re-derived from the new theme, so switching
/// mid-frame is safe. It also registers [`sync_pixels_per_point`] to run at
/// the start of every pass.
pub fn switch(ctx: &Context, theme: Arc<Theme>) {
    super::apply_theme(ctx, &theme);
    install(ctx, theme);
    ctx.add_plugin(PixelsPerPointSync);
}

/// Store `theme` as the active theme of `ctx` and bump [`generation`],
/// without touching egui's visuals. Only [`switch`] calls this.
fn install(ctx: &Context, theme: Arc<Theme>) {
    with_state(ctx, |state| {
        let mut parent = Arc::clone(&theme);
        for (scope, scoped) in &mut state.stack {
            *scoped = parent.scoped(*scope);
            parent = Arc::clone(scoped);
        }
        state.installed = Some(theme);
        state.generation += 1;
    });
}

/// Record that egui's visuals were built for `pixels_per_point`.
pub(super) fn record_applied_pixels_per_point(ctx: &Context, pixels_per_point: f32) {
    with_state(ctx, |state| {
        state.applied_pixels_per_point = Some(pixels_per_point);
    });
}

/// Rebuild egui's visuals when the display scale changed since the theme was
/// last applied, and return whether it did.
///
/// Some visuals depend on the scale: CSS borders snap to whole device pixels
/// ([`crate::design::size::css_border_width`]), so a 1.5 point control border
/// is 1 point wide at 1× and 1.5 points at 2×. This compares one float and
/// does nothing until a theme has been applied. [`switch`] registers it to
/// run at the start of every pass of every viewport, so moving a window to a
/// screen with another scale (or zooming) re-applies the theme.
pub fn sync_pixels_per_point(ctx: &Context) -> bool {
    let pixels_per_point = ctx.pixels_per_point();
    let stale = with_state(ctx, |state| {
        state
            .applied_pixels_per_point
            .is_some_and(|applied| applied != pixels_per_point)
    });
    if stale {
        let theme = with_state(ctx, |state| state.base());
        super::apply_theme(ctx, &theme);
    }
    stale
}

/// Runs [`sync_pixels_per_point`] at the start of every pass.
struct PixelsPerPointSync;

impl egui::Plugin for PixelsPerPointSync {
    fn debug_name(&self) -> &'static str {
        "components::design::theme::PixelsPerPointSync"
    }

    fn on_begin_pass(&mut self, ui: &mut Ui) {
        if sync_pixels_per_point(ui.ctx()) {
            // The root `Ui` of this pass was created with the old style.
            ui.set_style(ui.ctx().global_style());
        }
    }
}

/// The theme for the innermost active [`scope`], or the installed theme, or
/// the built-in default when none has been installed.
pub fn current(ctx: &Context) -> Arc<Theme> {
    with_state(ctx, |state| state.current())
}

/// A counter that increases every time a theme is installed.
///
/// Caches of theme-derived values (highlighted text, tessellated icons)
/// include it in their key so a live theme switch invalidates them.
pub fn generation(ctx: &Context) -> u64 {
    with_state(ctx, |state| state.generation)
}

/// Run `add_contents` with the theme re-pointed for `scope`.
///
/// The scoped theme is pushed onto a context-wide stack for the duration of
/// the call and popped afterwards, also when `add_contents` panics, so the
/// stack stays balanced.
///
/// Stock egui text edits read their background from `ui`'s visuals rather
/// than the theme, so for the duration of the call those
/// (`extreme_bg_color`, `text_edit_bg_color`) are set to the scoped
/// `field-bg` as well, and restored afterwards.
pub fn scope<R>(ui: &mut Ui, scope: ThemeScope, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    let _guard = ScopeGuard::enter(ui.ctx(), scope);
    let field = current(ui.ctx()).components.field_bg;
    let visuals = ui.visuals();
    let saved = (visuals.extreme_bg_color, visuals.text_edit_bg_color);
    if saved == (field, Some(field)) {
        return add_contents(ui);
    }
    set_field_background(ui, (field, Some(field)));
    let result = add_contents(ui);
    set_field_background(ui, saved);
    result
}

fn set_field_background(ui: &mut Ui, (extreme, text_edit): (egui::Color32, Option<egui::Color32>)) {
    let visuals = ui.visuals_mut();
    visuals.extreme_bg_color = extreme;
    visuals.text_edit_bg_color = text_edit;
}

/// Pops the scope stack back to its depth before [`scope`] pushed onto it.
struct ScopeGuard {
    ctx: Context,
    depth: usize,
}

impl ScopeGuard {
    fn enter(ctx: &Context, scope: ThemeScope) -> Self {
        let depth = with_state(ctx, |state| {
            let depth = state.stack.len();
            let scoped = state.current().scoped(scope);
            state.stack.push((scope, scoped));
            depth
        });
        Self {
            ctx: ctx.clone(),
            depth,
        }
    }
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        with_state(&self.ctx, |state| state.stack.truncate(self.depth));
    }
}

/// Read the active theme from an egui [`Context`] or [`Ui`].
///
/// Named `tokens` because `egui::Context::theme` already exists (it returns
/// egui's dark/light mode) and inherent methods shadow trait methods.
pub trait ThemeExt {
    /// The theme for the current scope. See [`current`].
    fn tokens(&self) -> Arc<Theme>;
}

impl ThemeExt for Context {
    fn tokens(&self) -> Arc<Theme> {
        current(self)
    }
}

impl ThemeExt for Ui {
    fn tokens(&self) -> Arc<Theme> {
        current(self.ctx())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::palette;
    use crate::design::theme::ThemeDefinition;

    fn run(ctx: &Context, mut f: impl FnMut(&mut Ui)) {
        let mut output = ctx.run_ui(Default::default(), |ui| f(ui));
        output.textures_delta.clear();
    }

    fn custom_theme(name: &str) -> Arc<Theme> {
        let mut definition = ThemeDefinition::default();
        definition.set_name(name);
        definition.set_palette(palette::PaletteKey::Sand875, palette::MINT_900);
        Arc::new(definition.resolve())
    }

    #[test]
    fn falls_back_to_builtin_theme_until_one_is_installed() {
        let ctx = Context::default();
        assert_eq!(ctx.tokens().name(), "Harbour Dark");
        assert_eq!(generation(&ctx), 0);

        install(&ctx, custom_theme("Custom"));
        assert_eq!(generation(&ctx), 1);
        assert_eq!(ctx.tokens().name(), "Custom");
        assert_eq!(ctx.tokens().surface.panel, palette::MINT_900);

        install(&ctx, Arc::new(Theme::default()));
        assert_eq!(generation(&ctx), 2);
        assert_eq!(ctx.tokens().name(), "Harbour Dark");
    }

    #[test]
    fn scopes_nest_and_pop_within_a_frame() {
        let ctx = Context::default();
        run(&ctx, |ui| {
            let root = ui.tokens();
            assert_eq!(root.components.row_bg, root.surface.panel);

            let inner = scope(ui, ThemeScope::Blade, |ui| {
                assert_eq!(ui.tokens().components.row_bg, root.surface.raised);
                scope(ui, ThemeScope::Card, |ui| ui.tokens())
            });
            assert_eq!(inner.components.field_bg, root.surface.panel);

            assert_eq!(ui.tokens().components.row_bg, root.surface.panel);
        });
        assert_eq!(with_state(&ctx, |state| state.stack.len()), 0);
    }

    #[test]
    fn scope_stack_stays_balanced_when_contents_panic() {
        let ctx = Context::default();
        run(&ctx, |ui| {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                scope(ui, ThemeScope::Dialog, |ui| {
                    scope(ui, ThemeScope::Popover, |_| panic!("widget failed"));
                });
            }));
            assert!(outcome.is_err());
            assert_eq!(with_state(ui.ctx(), |state| state.stack.len()), 0);
            assert_eq!(ui.tokens().components.row_bg, ui.tokens().surface.panel);
        });
    }

    #[test]
    fn scopes_point_stock_text_edits_at_the_scoped_field_background() {
        let ctx = Context::default();
        crate::design::theme::configure_default(&ctx);
        // The fonts become active on the next pass.
        run(&ctx, |_| {});
        run(&ctx, |ui| {
            let root = ui.tokens();
            assert_eq!(ui.visuals().extreme_bg_color, root.components.field_bg);

            scope(ui, ThemeScope::Popover, |ui| {
                // `.pop` re-points fields at the panel tone.
                assert_eq!(ui.visuals().extreme_bg_color, root.surface.panel);
                assert_eq!(ui.visuals().text_edit_bg_color, Some(root.surface.panel));
                scope(ui, ThemeScope::Sidebar, |ui| {
                    assert_eq!(ui.visuals().extreme_bg_color, root.surface.raised);
                });
                assert_eq!(ui.visuals().extreme_bg_color, root.surface.panel);

                // A stock text edit paints the scoped field background.
                let mut text = String::from("value");
                let response = ui.add(egui::TextEdit::singleline(&mut text));
                let painted = ui.ctx().graphics(|graphics| {
                    graphics.get(response.layer_id).is_some_and(|shapes| {
                        shapes.all_entries().any(|clipped| {
                            matches!(&clipped.shape, egui::Shape::Rect(rect)
                                    if rect.fill == root.surface.panel
                                        && rect.rect.contains_rect(response.rect.shrink(1.0)))
                        })
                    })
                });
                assert!(
                    painted,
                    "the text edit frame uses the popover field background"
                );
            });

            assert_eq!(ui.visuals().extreme_bg_color, root.components.field_bg);
            assert_eq!(
                ui.visuals().text_edit_bg_color,
                Some(root.components.field_bg)
            );
        });
    }

    #[test]
    fn installing_inside_a_scope_rederives_the_scope() {
        let ctx = Context::default();
        let replacement = custom_theme("Live switch");
        run(&ctx, |ui| {
            scope(ui, ThemeScope::Card, |ui| {
                install(ui.ctx(), Arc::clone(&replacement));
                let scoped = ui.tokens();
                assert_eq!(scoped.name(), "Live switch");
                assert_eq!(scoped.components.row_bg, replacement.surface.raised);
            });
            assert_eq!(ui.tokens().name(), "Live switch");
            assert_eq!(ui.tokens().components.row_bg, replacement.surface.panel);
        });
    }
}
