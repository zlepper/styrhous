//! AccessKit semantics of the containers the components draw themselves
//! (dialogs, blades, toasts, banners, the sidebar), which egui exposes as
//! nameless, boundless [`Ui`] nodes.

use egui::accesskit::Role;
use egui::{Id, Rect, Ui};

/// How a container is named.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Name<'a> {
    /// By these words.
    Label(&'a str),
    /// By the label widget `Id` inside it (its title), so the words are not
    /// read or found twice.
    LabelledBy(Id),
}

/// Give the AccessKit node of the container `ui` its `role`, `name` and,
/// when known, the `bounds` it paints.
pub(crate) fn describe_container(ui: &Ui, role: Role, name: Name<'_>, bounds: Option<Rect>) {
    ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
        node.set_role(role);
        match name {
            Name::Label(label) => node.set_label(label),
            Name::LabelledBy(label) => node.set_labelled_by(vec![label.accesskit_id()]),
        }
        if let Some(rect) = bounds {
            node.set_bounds(egui::accesskit::Rect {
                x0: rect.min.x.into(),
                y0: rect.min.y.into(),
                x1: rect.max.x.into(),
                y1: rect.max.y.into(),
            });
        }
    });
}
