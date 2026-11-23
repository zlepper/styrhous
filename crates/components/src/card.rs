//! Cards (`.card`): a `surface-panel` panel with a padded body, and the
//! [`Disclosure`] card whose header toggles its body (C13).
//!
//! A card fills the width it is given, like a CSS block. Its body
//! (`.card-b`) is padded [`space::S16`]; a header row (`.card-h`) is
//! [`size::CARD_HEADER`] tall with a divider between it and the body.

use egui::{Color32, Galley, Margin, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::design::theme::{self, ThemeExt, ThemeScope};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space, stroke};
use crate::icons::Icon;
use crate::interaction::PointingHand;

/// The title of a card header (`.card-h .w6`): 13/20 · 600.
const HEADER_TITLE: TextRole = type_scale::UI_STRONG;
/// Gap between the items of a card header.
const HEADER_GAP: f32 = space::S8;

/// A card (`.card`) for inspector sections, settings, filters and tables:
/// the card fill ([`Theme::card_fill`](crate::design::theme::Theme::card_fill))
/// with the card radius and no border, filling the available width like a
/// CSS block, its contents padded [`space::S16`] and shown in
/// [`ThemeScope::Card`], which re-points rows, fields and wells for the
/// card's background.
#[must_use = "call `show` to add the card"]
pub struct Card {
    padding: Margin,
    fill: Option<Color32>,
    stroke: Option<Stroke>,
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Card {
    pub fn new() -> Self {
        Self {
            padding: Margin::same(space::S16 as i8),
            fill: None,
            stroke: None,
        }
    }

    /// Pad the contents by `padding` instead (zero for rows that run to the
    /// card's edges, such as [`crate::ListRow`]s).
    pub fn padding(mut self, padding: impl Into<Margin>) -> Self {
        self.padding = padding.into();
        self
    }

    /// Another fill, for selected cards.
    pub fn fill(mut self, fill: Color32) -> Self {
        self.fill = Some(fill);
        self
    }

    /// A visible outline for a selected or highlighted card, drawn inside
    /// its edge so the card keeps its size.
    pub fn stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = Some(stroke);
        self
    }

    pub fn show<R>(self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        self.show_inner(ui, add_contents).inner
    }

    /// Like [`Self::show`], also returning the card's response.
    pub fn show_inner<R>(
        self,
        ui: &mut Ui,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> egui::InnerResponse<R> {
        let theme = ui.tokens();
        let inner = egui::Frame::new()
            .fill(self.fill.unwrap_or_else(|| theme.card_fill()))
            .corner_radius(radius::CARD)
            .inner_margin(self.padding)
            .show(ui, |ui| {
                theme::scope(ui, ThemeScope::Card, |ui| {
                    ui.set_min_width(ui.available_width());
                    add_contents(ui)
                })
            });
        if let Some(stroke) = self.stroke {
            ui.painter().rect_stroke(
                inner.response.rect,
                radius::CARD,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        inner
    }
}

/// The response of a [`Disclosure`].
pub struct DisclosureResponse<R> {
    /// The header, the disclosure's toggle.
    pub header: Response,
    /// Whether the body is shown.
    pub open: bool,
    /// What the body returned, when shown.
    pub inner: Option<R>,
}

/// A disclosure card: a [`size::CARD_HEADER`] header (chevron, title, meta
/// count) that opens and closes the body below it. The whole header is the
/// toggle; Enter and Space toggle it when focused.
///
/// Whether it is open is remembered (across restarts) under its id; give
/// disclosures of the same section of every resource of one kind the same
/// id so they open and close together.
#[must_use = "call `show` to add the disclosure"]
pub struct Disclosure<'a> {
    id: egui::Id,
    title: &'a str,
    meta: Option<&'a str>,
    default_open: bool,
    forced_open: Option<bool>,
    persist: bool,
}

impl<'a> Disclosure<'a> {
    /// A closed disclosure titled `title`, remembered under `id_salt`.
    pub fn new(id_salt: impl std::hash::Hash + std::fmt::Debug, title: &'a str) -> Self {
        Self {
            id: egui::Id::new(("disclosure", id_salt)),
            title,
            meta: None,
            default_open: false,
            forced_open: None,
            persist: true,
        }
    }

    /// Muted text after the title, such as "6 labels · 3 annotations".
    pub fn meta(mut self, meta: &'a str) -> Self {
        self.meta = Some(meta);
        self
    }

    /// Open the disclosure the first time it is shown.
    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }

    /// Remember whether it is open only for this session, not across
    /// restarts (for sections that may reveal sensitive values).
    pub fn session_only(mut self) -> Self {
        self.persist = false;
        self
    }

    /// Show it open or closed regardless of its remembered state (design
    /// boards and snapshots).
    pub fn force_open(mut self, open: bool) -> Self {
        self.forced_open = Some(open);
        self
    }

    pub fn show<R>(
        self,
        ui: &mut Ui,
        add_body: impl FnOnce(&mut Ui) -> R,
    ) -> DisclosureResponse<R> {
        let id = self.id;
        let stored = if self.persist {
            ui.data_mut(|data| data.get_persisted::<bool>(id))
        } else {
            ui.data(|data| data.get_temp::<bool>(id))
        }
        .unwrap_or(self.default_open);
        let open = self.forced_open.unwrap_or(stored);
        // The body keeps the spacing of the surrounding layout.
        let body_spacing = ui.spacing().item_spacing;
        let persist = self.persist;
        let forced = self.forced_open.is_some();
        let inner = Card::new().padding(0).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let header = header_row(ui, self.title, self.meta, open);
            let open = if header.clicked() { !open } else { open };
            if open != stored && !forced {
                ui.data_mut(|data| {
                    if persist {
                        data.insert_persisted(id, open);
                    } else {
                        data.insert_temp(id, open);
                    }
                });
            }
            let body = open.then(|| {
                let theme = ui.tokens();
                let (line, _) = ui.allocate_exact_size(
                    vec2(ui.available_width(), size::HAIRLINE),
                    Sense::hover(),
                );
                ui.painter()
                    .hline(line.x_range(), line.center().y, stroke::divider(&theme));
                egui::Frame::new()
                    .inner_margin(Margin::same(space::S16 as i8))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.spacing_mut().item_spacing = body_spacing;
                        add_body(ui)
                    })
                    .inner
            });
            (header, open, body)
        });
        let (header, open, inner) = inner;
        DisclosureResponse {
            header,
            open: self.forced_open.unwrap_or(open),
            inner,
        }
    }
}

/// The clickable header row of a disclosure: chevron, title and meta,
/// [`HEADER_GAP`] apart and centred, [`space::S16`] in from the edges.
fn header_row(ui: &mut Ui, title: &str, meta: Option<&str>, open: bool) -> Response {
    let theme = ui.tokens();
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), size::CARD_HEADER),
        Sense::click(),
    );
    let response = response.with_pointing_hand();
    // Read as its title, the meta count as its description; the words are
    // the header's own, not labels of their own.
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, open, title)
    });
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_expanded(open);
        if let Some(meta) = meta {
            node.set_description(meta);
        }
    });
    let center_y = rect.center().y;
    let mut x = rect.left() + space::S16;
    let chevron = if open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    if ui.is_rect_visible(rect) {
        chevron.paint(
            ui,
            Rect::from_center_size(
                pos2(x + size::ICON_SM / 2.0, center_y),
                Vec2::splat(size::ICON_SM),
            ),
            theme.text.muted,
        );
    }
    x += size::ICON_SM + HEADER_GAP;
    let centred = |galley: &Galley, x: f32| pos2(x, center_y - galley.rect.height() / 2.0);
    let title_galley = HEADER_TITLE.galley(ui.ctx(), title, theme.text.primary, f32::INFINITY);
    let title_pos = centred(&title_galley, x);
    x += title_galley.rect.width() + HEADER_GAP;
    type_scale::paint_text_as(
        ui,
        &response,
        title_pos,
        title_galley,
        theme.text.primary,
        egui::accesskit::Role::Button,
    );
    if let Some(meta) = meta
        && ui.is_rect_visible(rect)
    {
        let galley = type_scale::META.galley(ui.ctx(), meta, theme.text.muted, f32::INFINITY);
        let pos = centred(&galley, x);
        type_scale::paint_galley(ui.painter(), pos, galley, theme.text.muted);
    }
    response
}

/// A single-line label whose CSS line box is centred in the row.
pub(crate) fn centred_label(
    ui: &mut Ui,
    role: TextRole,
    text: &str,
    color: egui::Color32,
) -> Response {
    let galley = role.galley(ui.ctx(), text, color, f32::INFINITY);
    let row = ui.max_rect();
    let top = row.center().y - galley.rect.height() / 2.0;
    let rect = Rect::from_min_size(pos2(ui.cursor().left(), top), galley.rect.size());
    let response = type_scale::allocate_text_at(ui, rect, text);
    type_scale::paint_text(ui, &response, rect.min, galley, color);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::setup_egui;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn a_disclosure_toggles_from_its_header_and_remembers_it() {
        let mut harness = Harness::builder().build_ui(|ui| {
            let response = Disclosure::new("labels", "Labels & annotations")
                .meta("6 labels")
                .show(ui, |ui| {
                    type_scale::UI.label(ui, "body", ui.tokens().text.primary);
                });
            assert_eq!(response.open, response.inner.is_some());
        });
        setup_egui(&mut harness);
        harness.run();
        assert!(harness.query_by_label("body").is_none());
        harness.get_by_label("Labels & annotations").click();
        harness.run();
        harness.run();
        assert!(harness.query_by_label("body").is_some());
        harness.get_by_label("Labels & annotations").click();
        harness.run();
        harness.run();
        assert!(harness.query_by_label("body").is_none());
    }
}
