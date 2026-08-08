use super::state::ResourceAction;
use crate::minimal_resource::MinimalResource;
use components::colors::gray;
use components::{MoreMenu, icons};

/// Render the shared resource-level actions used by table rows and inspectors.
pub(super) fn show_resource_action_items(
    menu: &mut MoreMenu<'_>,
    resource: &MinimalResource,
    pending_action: &mut Option<ResourceAction>,
) {
    if menu
        .action_with_icon(
            "Edit YAML",
            icons::document_icon()
                .fit_to_exact_size(egui::Vec2::splat(16.0))
                .tint(gray::_500),
        )
        .clicked()
        && pending_action.is_none()
    {
        *pending_action = Some(ResourceAction::EditYaml {
            name: resource.name.clone(),
            namespace: resource.namespace.clone(),
        });
    }
    menu.separator();
    if menu
        .destructive_action_with_icon(
            "Delete",
            icons::trash_icon()
                .fit_to_exact_size(egui::Vec2::splat(16.0))
                .tint(egui::Color32::from_rgb(185, 28, 28)),
        )
        .clicked()
        && pending_action.is_none()
    {
        *pending_action = Some(ResourceAction::RequestDelete {
            name: resource.name.clone(),
            namespace: resource.namespace.clone(),
        });
    }
}
