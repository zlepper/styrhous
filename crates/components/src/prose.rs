//! Prose: a paragraph with emphasised runs, such as the resource a dialog
//! names in bold (`<b>coredns</b>`).

use std::borrow::Cow;
use std::sync::Arc;

use egui::text::LayoutJob;
use egui::{Color32, Context, Galley, Response, Ui};

use crate::design::type_scale::{self, TextRole, Weight};

/// One run of a [`Prose`] paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Run<'a> {
    text: Cow<'a, str>,
    strong: bool,
}

/// A paragraph of plain and strong runs, set in one [`TextRole`]. Strong
/// runs use the role's typeface at [`Weight::Bold`] (CSS `<b>`).
///
/// ```ignore
/// Prose::new()
///     .plain("Your unsaved edits to ")
///     .strong("settings")
///     .plain(" will be lost.")
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prose<'a> {
    runs: Vec<Run<'a>>,
}

impl<'a> Prose<'a> {
    /// An empty paragraph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append `text` in the role's own weight.
    pub fn plain(self, text: impl Into<Cow<'a, str>>) -> Self {
        self.run(text, false)
    }

    /// Append `text` in bold.
    pub fn strong(self, text: impl Into<Cow<'a, str>>) -> Self {
        self.run(text, true)
    }

    fn run(mut self, text: impl Into<Cow<'a, str>>, strong: bool) -> Self {
        let text = text.into();
        if !text.is_empty() {
            self.runs.push(Run { text, strong });
        }
        self
    }

    /// Whether the paragraph has no text.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// The paragraph as plain text, for accessible names and descriptions.
    pub fn text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_ref()).collect()
    }

    /// The paragraph as a layout job in `role`, wrapping at `wrap_width`.
    pub fn layout_job(
        &self,
        ctx: &Context,
        role: TextRole,
        color: Color32,
        wrap_width: f32,
    ) -> LayoutJob {
        let plain = role.text_format(ctx, color);
        let strong = role.with_weight(Weight::Bold).text_format(ctx, color);
        let mut job = LayoutJob::default();
        for run in &self.runs {
            let format = if run.strong { &strong } else { &plain };
            job.append(&role.apply_case(&run.text), 0.0, format.clone());
        }
        job.wrap.max_width = wrap_width;
        job
    }

    /// The paragraph in CSS line boxes ([`TextRole::galley_from_job`]).
    pub fn galley(
        &self,
        ctx: &Context,
        role: TextRole,
        color: Color32,
        wrap_width: f32,
    ) -> Arc<Galley> {
        role.galley_from_job(ctx, self.layout_job(ctx, role, color, wrap_width))
    }

    /// Paint the paragraph as a block filling the available width, read as
    /// one label ([`TextRole::label`]).
    pub fn label(&self, ui: &mut Ui, role: TextRole, color: Color32) -> Response {
        let galley = self.galley(ui.ctx(), role, color, ui.available_width());
        let (rect, response) = type_scale::allocate_text(ui, galley.rect.size(), galley.text());
        type_scale::paint_text(ui, &response, rect.min, galley, color);
        response
    }
}

impl<'a> From<&'a str> for Prose<'a> {
    fn from(text: &'a str) -> Self {
        Prose::new().plain(text)
    }
}

impl<'a> From<&'a String> for Prose<'a> {
    fn from(text: &'a String) -> Self {
        Prose::new().plain(text.as_str())
    }
}

impl From<String> for Prose<'static> {
    fn from(text: String) -> Self {
        Prose::new().plain(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_concatenate_into_plain_text() {
        let prose = Prose::new()
            .plain("Removes every finalizer from ")
            .strong("important-config")
            .plain(".")
            .strong("");
        assert_eq!(
            prose.text(),
            "Removes every finalizer from important-config."
        );
        assert_eq!(prose.runs.len(), 3);
        assert!(Prose::from("").is_empty());
    }

    #[test]
    fn strong_runs_use_the_bold_face() {
        let ctx = Context::default();
        crate::design::fonts::install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        let color = crate::design::theme::Theme::default().text.primary;
        let prose = Prose::new().plain("in ").strong("kube-system");
        let job = prose.layout_job(&ctx, type_scale::BODY, color, 100.0);
        assert_eq!(job.sections.len(), 2);
        assert_eq!(
            job.sections[1].format.font_id,
            type_scale::BODY.with_weight(Weight::Bold).font_id(&ctx)
        );
        assert_eq!(job.wrap.max_width, 100.0);
    }
}
