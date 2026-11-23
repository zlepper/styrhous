//! SVG icons for the components library.
//!
//! The outline icons are the design's own set — Heroicons outline, stroke
//! width 1.5 — extracted byte for byte from the Harbour stylesheet by
//! `scripts/design-oracles/extract-icons.mjs` into `assets/icons/`, so egui
//! and the design oracles rasterize identical paths. They are drawn in white
//! and tinted with the text colour of their context. A few app-only icons
//! (Heroicons the design does not use, and provider brand marks) live next
//! to them.
//!
//! Every icon is an [`Icon`] variant, named after its SVG file; each also has
//! a `<name>_icon()` function returning its [`Image`]. Icons are drawn at the
//! design's sizes (12, 14, 16 by default, 20 and 24 points: `ICON_XS` to `ICON_XL` in
//! [`crate::design::size`]).
//!
//! Icons render through the egui_extras SVG loader, which rasterizes at the
//! exact pixel size the image is shown at. The consuming application must
//! install the image loaders once at startup:
//! ```ignore
//! egui_extras::install_image_loaders(&cc.egui_ctx);
//! ```
//!
//! # Usage
//!
//! ```ignore
//! Icon::ChevronRight.show(ui, size::ICON, ui.tokens().text.muted);
//! ```

use egui::{Color32, Image, Rect, Response, Ui, Vec2, include_image};

use crate::PointingHand;

macro_rules! icons {
    (
        design { $($(#[$design_doc:meta])* $design:ident => $design_file:literal;)+ }
        app { $($(#[$app_doc:meta])* $app:ident => $app_file:literal;)+ }
    ) => {
        /// An icon of the bundled set, named after its SVG file.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon {
            $($(#[$design_doc])* $design,)+
            $($(#[$app_doc])* $app,)+
        }

        impl Icon {
            /// The design's icon set (the `.ic-*` masks of the Harbour
            /// stylesheet), in stylesheet order.
            pub const DESIGN: &[Self] = &[$(Self::$design,)+];
            /// Every bundled icon: the design set, then the app-only icons.
            pub const ALL: &[Self] = &[$(Self::$design,)+ $(Self::$app,)+];

            /// The SVG file stem, which is also the design's icon name, for
            /// example `"arrow-left"`.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$design => $design_file,)+
                    $(Self::$app => $app_file,)+
                }
            }

            /// The icon as an egui image, white until tinted.
            pub fn image(self) -> Image<'static> {
                match self {
                    $(Self::$design => Image::new(include_image!(concat!("../assets/icons/", $design_file, ".svg"))),)+
                    $(Self::$app => Image::new(include_image!(concat!("../assets/icons/", $app_file, ".svg"))),)+
                }
            }
        }

    };
}

icons! {
    design {
        /// Left arrow, for backwards navigation.
        ArrowLeft => "arrow-left";
        /// Right arrow, for forwards navigation.
        ArrowRight => "arrow-right";
        /// Up arrow.
        ArrowUp => "arrow-up";
        /// Down arrow.
        ArrowDown => "arrow-down";
        /// Circular arrow, for reload actions.
        ArrowPath => "arrow-path";
        /// Chevron down.
        ChevronDown => "chevron-down";
        /// Chevron up.
        ChevronUp => "chevron-up";
        /// Chevron right, for navigation affordances.
        ChevronRight => "chevron-right";
        /// Chevron left.
        ChevronLeft => "chevron-left";
        /// Up-down chevrons, for select triggers.
        ChevronUpDown => "chevron-up-down";
        /// Double chevron down, for jumping to the end.
        ChevronDoubleDown => "chevron-double-down";
        /// X mark, for dismissing overlays and panels.
        XMark => "x-mark";
        /// Check mark.
        Check => "check";
        /// Minus.
        Minus => "minus";
        /// Plus, for add controls.
        Plus => "plus";
        /// Magnifying glass.
        MagnifyingGlass => "magnifying-glass";
        /// Funnel, for filtering controls.
        Funnel => "funnel";
        /// Numbered list, for line-number visibility controls.
        NumberedList => "numbered-list";
        /// Calendar days, for timestamp visibility controls.
        CalendarDays => "calendar-days";
        /// Swatch, for ANSI styling controls.
        Swatch => "swatch";
        /// Tag, for source labels.
        Tag => "tag";
        /// Document text, for Kubernetes field references.
        DocumentText => "document-text";
        /// Horizontal ellipsis, for compact action menus.
        EllipsisHorizontal => "ellipsis-horizontal";
        /// Pencil on a square, for opening an editor.
        PencilSquare => "pencil-square";
        /// Pencil, for edit actions.
        Pencil => "pencil";
        /// Trash, for destructive actions.
        Trash => "trash";
        /// Bell, for the session operation history.
        Bell => "bell";
        /// Cog, for application settings.
        Cog6Tooth => "cog-6-tooth";
        /// Command line, for shells and terminals.
        CommandLine => "command-line";
        /// Clipboard document, for copying.
        ClipboardDocument => "clipboard-document";
        /// Document duplicate, for copy actions.
        DocumentDuplicate => "document-duplicate";
        /// Eye, for revealing values.
        Eye => "eye";
        /// Information circle.
        InformationCircle => "information-circle";
        /// Exclamation triangle, for warnings.
        ExclamationTriangle => "exclamation-triangle";
        /// Exclamation circle, for errors.
        ExclamationCircle => "exclamation-circle";
        /// Check circle, for completed operations.
        CheckCircle => "check-circle";
        /// X circle, for failed operations.
        XCircle => "x-circle";
        /// Closed lock.
        LockClosed => "lock-closed";
        /// Server stack: nodes.
        ServerStack => "server-stack";
        /// Cube, a Kubernetes resource.
        Cube => "cube";
        /// Globe.
        GlobeAlt => "globe-alt";
        /// Folder.
        Folder => "folder";
        /// Circle stack: contexts and clusters.
        CircleStack => "circle-stack";
        /// Horizontal adjustments: presentation settings.
        AdjustmentsHorizontal => "adjustments-horizontal";
        /// Share: cluster discovery.
        Share => "share";
        /// User circle: license and account.
        UserCircle => "user-circle";
        /// Arrow into a tray, for downloads.
        ArrowDownTray => "arrow-down-tray";
        /// Play.
        Play => "play";
        /// Clock, for timestamps.
        Clock => "clock";
        /// Queue list.
        QueueList => "queue-list";
        /// Cloud.
        Cloud => "cloud";
        /// Link.
        Link => "link";
        /// Rectangle stack.
        RectangleStack => "rectangle-stack";
        /// Arrow out of a square, for opening in a new window.
        ArrowTopRightOnSquare => "arrow-top-right-on-square";
        /// Key, for secret-backed values.
        Key => "key";
        /// Three bars, for menus.
        Bars3 => "bars-3";
        /// Up and down arrows, for sorting.
        ArrowsUpDown => "arrows-up-down";
        /// Two-by-two squares.
        Squares2x2 => "squares-2x2";
        /// Wrench and screwdriver: application preferences.
        WrenchScrewdriver => "wrench-screwdriver";
        /// Signal.
        Signal => "signal";
        /// U-turn arrow, for undo.
        ArrowUturnLeft => "arrow-uturn-left";
    }
    app {
        /// Home.
        Home => "home";
        /// Users.
        Users => "users";
        /// Calendar.
        Calendar => "calendar";
        /// Document.
        Document => "document";
        /// Code bracket, for literal values.
        CodeBracket => "code-bracket";
        /// Question mark, for unknown or unspecified values.
        QuestionMarkCircle => "question-mark-circle";
        /// Bar chart.
        ChartBar => "chart-bar";
        /// The bundled Microsoft Azure brand mark for Azure provider surfaces.
        Azure => "azure";
        /// The bundled Tailscale brand mark for Tailscale provider surfaces.
        Tailscale => "tailscale";
    }
}

impl Icon {
    /// Show the icon `size` points square, tinted `color` (the text colour of
    /// its context).
    pub fn show(self, ui: &mut Ui, size: f32, color: Color32) -> Response {
        ui.add(
            self.image()
                .fit_to_exact_size(Vec2::splat(size))
                .tint(color),
        )
    }

    /// Paint the icon into `rect`, tinted `color`.
    pub fn paint(self, ui: &Ui, rect: Rect, color: Color32) {
        self.image().tint(color).paint_at(ui, rect);
    }
}

/// A clickable, frameless `icon` button `size` points square, tinted
/// `color` and labelled `label` for assistive technologies.
pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32, color: Color32, label: &str) -> Response {
    let image = icon
        .image()
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color);
    let response = ui
        .add(egui::Button::image(image).frame(false))
        .with_pointing_hand();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_design_set_is_complete_and_uniquely_named() {
        assert_eq!(
            Icon::DESIGN.len(),
            61,
            "the Harbour stylesheet has 61 icons"
        );
        let names: std::collections::HashSet<_> =
            Icon::ALL.iter().map(|icon| icon.name()).collect();
        assert_eq!(names.len(), Icon::ALL.len());
        assert_eq!(Icon::Cog6Tooth.name(), "cog-6-tooth");
    }
}
