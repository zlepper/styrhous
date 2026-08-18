//! Type-erased content used by the application's shared blade history.
//!
//! A blade's visual stack is intentionally generic: adding a new kind of
//! content must not require growing a central enum.  Content implementations
//! expose the small set of capabilities their owners need through optional
//! hooks instead.

use super::state::{ResourceDetailHistoryEntry, UiState};
use super::table_preferences::PersistedResourceTablePreferences;
use crate::terminal_launcher::ShellRequest;
use crate::terminal_launcher::{DebugImagePreset, TerminalLaunchSettings};
use crate::updater::UpdateStatus;
use crate::worker::WorkerCommandBox;
use components::{BladeLayer, BladeNavigator, BladeResponse, BladeStack};
use std::cell::RefCell;

/// The application's shared blade renderer.
pub(super) struct GlobalBladeCoordinator {
    stack: BladeStack,
    navigator: Option<BladeNavigator<Box<dyn GlobalBladeContent>>>,
}

impl Default for GlobalBladeCoordinator {
    fn default() -> Self {
        Self {
            stack: BladeStack::new("global-blade-stack"),
            navigator: None,
        }
    }
}

impl GlobalBladeCoordinator {
    /// Replaces the active root blade. The previous history is returned to
    /// `UiState::replace_global_blade`, the sole application entry point that
    /// releases resources owned by discarded entries.
    pub(super) fn open(
        &mut self,
        content: Box<dyn GlobalBladeContent>,
    ) -> Vec<Box<dyn GlobalBladeContent>> {
        let discarded = self
            .navigator
            .take()
            .into_iter()
            .flat_map(BladeNavigator::into_entries)
            .collect();
        self.navigator = Some(BladeNavigator::new(content));
        discarded
    }

    #[cfg(test)]
    pub(super) fn push(
        &mut self,
        content: Box<dyn GlobalBladeContent>,
    ) -> Vec<Box<dyn GlobalBladeContent>> {
        self.navigator
            .as_mut()
            .expect("a blade must be open before adding history")
            .push(content)
    }

    pub(super) fn navigator(&self) -> Option<&BladeNavigator<Box<dyn GlobalBladeContent>>> {
        self.navigator.as_ref()
    }

    pub(super) fn navigator_mut(
        &mut self,
    ) -> Option<&mut BladeNavigator<Box<dyn GlobalBladeContent>>> {
        self.navigator.as_mut()
    }

    pub(super) fn clear(&mut self) -> Vec<Box<dyn GlobalBladeContent>> {
        self.navigator
            .take()
            .into_iter()
            .flat_map(BladeNavigator::into_entries)
            .collect()
    }

    fn show_contents(
        &self,
        ctx: &egui::Context,
        navigator: &mut BladeNavigator<Box<dyn GlobalBladeContent>>,
        render_context: GlobalBladeRenderContext<'_>,
    ) -> BladeResponse<GlobalBladeRenderResult, GlobalBladeRenderResult> {
        let render_context = RefCell::new(render_context);
        self.stack.show(
            ctx,
            navigator,
            |ui, content, layer| content.render_header(ui, layer, &mut render_context.borrow_mut()),
            |ui, content, layer| content.render_body(ui, layer, &mut render_context.borrow_mut()),
        )
    }

    pub(super) fn seed_transition(
        &self,
        ctx: &egui::Context,
        navigator: &mut BladeNavigator<Box<dyn GlobalBladeContent>>,
    ) {
        self.stack.seed_transition(ctx, navigator);
    }

    /// Render and advance the sole global blade navigator.  The coordinator
    /// retains ownership of the navigator for the entire lifecycle, so no
    /// feature module can accidentally create a second stack or lose cleanup
    /// while a child blade is foregrounded.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        ui_state: &mut UiState,
        commands_to_send: &mut Vec<WorkerCommandBox>,
        shell_requests: &mut Vec<ShellRequest>,
        debug_image_presets: &[DebugImagePreset],
        table_preferences: &mut PersistedResourceTablePreferences,
        terminal_launch_settings: &mut TerminalLaunchSettings,
        update_status: &UpdateStatus,
    ) {
        let Some(mut navigator) = self.navigator.take() else {
            return;
        };
        let resource_detail_cluster_keys = navigator
            .entries()
            .filter_map(|content| content.resource_detail().map(|entry| entry.cluster_key))
            .collect::<Vec<_>>();
        let dismiss_on_outside_click = resource_detail_cluster_keys
            .first()
            .and_then(|cluster_key| ui_state.clusters.get_mut(cluster_key))
            .and_then(|cluster| cluster.resource_detail_panel.as_mut())
            .map(|panel| {
                let dismiss = panel.dismiss_on_outside_click;
                panel.dismiss_on_outside_click = true;
                dismiss
            })
            .unwrap_or(false);
        let response = self.show_contents(
            ctx,
            &mut navigator,
            GlobalBladeRenderContext::new(
                ui_state,
                debug_image_presets,
                table_preferences,
                terminal_launch_settings,
                update_status,
            ),
        );

        let mut close = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        close |= (resource_detail_cluster_keys.is_empty() || dismiss_on_outside_click)
            && response.dismissed;
        close |= response.header.close || response.active.close;
        {
            let mut navigation = GlobalBladeNavigation {
                navigator: &mut navigator,
                commands_to_send,
            };
            if let Some(effect) = navigation.current_mut().take_effect() {
                effect.apply(
                    &mut GlobalBladeEffectContext {
                        ctx,
                        ui_state,
                        shell_requests,
                    },
                    &mut navigation,
                );
            }
            if let Some(next_content) = response.active.next_content {
                navigation.push(next_content);
            }
        }
        navigator.current_mut().show_overlay(ctx);
        if close {
            if navigator.begin_close() {
                self.seed_transition(ctx, &mut navigator);
            }
            self.navigator = Some(navigator);
        } else if response.close_finished {
            for cluster_key in resource_detail_cluster_keys {
                if let Some(cluster) = ui_state.clusters.get_mut(&cluster_key) {
                    cluster.resource_detail_panel = None;
                }
            }
            UiState::stop_discarded_blades(navigator.into_entries(), commands_to_send);
        } else {
            self.navigator = Some(navigator);
        }
    }
}

/// Render the single global blade history.  Individual content objects are
/// dynamically dispatched by `show_contents`; this host entry point keeps the
/// application frame loop independent of individual blade types.
#[allow(clippy::too_many_arguments)]
pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
    shell_requests: &mut Vec<ShellRequest>,
    debug_image_presets: &[DebugImagePreset],
    table_preferences: &mut PersistedResourceTablePreferences,
    terminal_launch_settings: &mut TerminalLaunchSettings,
    update_status: &UpdateStatus,
) {
    // Split borrows deliberately: the coordinator owns navigator state, while
    // content gets only the narrow render services it requires.
    let mut coordinator = std::mem::take(&mut ui_state.global_blades);
    coordinator.show(
        ctx,
        ui_state,
        commands_to_send,
        shell_requests,
        debug_image_presets,
        table_preferences,
        terminal_launch_settings,
        update_status,
    );
    // Effects normally operate through `GlobalBladeNavigation`, but preserve
    // an explicitly requested root replacement rather than overwriting it if
    // a future effect needs to replace the global blade while rendering.
    let replacement = std::mem::take(&mut ui_state.global_blades);
    if replacement.navigator().is_some() {
        UiState::stop_discarded_blades(coordinator.clear(), commands_to_send);
        coordinator = replacement;
    }
    ui_state.global_blades = coordinator;
}

#[cfg(test)]
mod tests {
    use super::{BladeLayer, GlobalBladeCoordinator};

    #[test]
    fn coordinator_is_the_single_entry_point_for_history_creation_and_pushes() {
        #[derive(Debug)]
        struct TestBlade;
        impl super::GlobalBladeContent for TestBlade {
            fn render_header(
                &mut self,
                _ui: &mut egui::Ui,
                _layer: BladeLayer,
                _context: &mut super::GlobalBladeRenderContext<'_>,
            ) -> super::GlobalBladeRenderResult {
                super::GlobalBladeRenderResult::default()
            }

            fn render_body(
                &mut self,
                _ui: &mut egui::Ui,
                _layer: BladeLayer,
                _context: &mut super::GlobalBladeRenderContext<'_>,
            ) -> super::GlobalBladeRenderResult {
                super::GlobalBladeRenderResult::default()
            }
        }
        let mut coordinator = GlobalBladeCoordinator::default();
        let discarded = coordinator.open(Box::new(TestBlade));
        assert!(discarded.is_empty());
        let discarded = coordinator.push(Box::new(TestBlade));

        assert!(discarded.is_empty());
        assert_eq!(coordinator.navigator().unwrap().back_stack().len(), 1);

        let discarded = coordinator.open(Box::new(TestBlade));
        assert_eq!(
            discarded.len(),
            2,
            "opening a new root discards the prior global history"
        );
        assert!(coordinator.navigator().unwrap().back_stack().is_empty());
    }
}

pub(super) trait GlobalBladeContent: std::fmt::Debug {
    /// Render this content's header and body through the one global stack.
    /// New blade types add their own implementation instead of extending the
    /// application frame loop or a central content enum.
    fn render_header(
        &mut self,
        _ui: &mut egui::Ui,
        _layer: BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult;

    fn render_body(
        &mut self,
        _ui: &mut egui::Ui,
        _layer: BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult;

    /// Return a deferred operation requested during rendering. Effects are
    /// dynamically dispatched after egui releases content borrows.
    fn take_effect(&mut self) -> Option<Box<dyn GlobalBladeEffect>> {
        None
    }

    /// Render an optional content-owned overlay after the blade frame. This
    /// keeps dialogs and other transient UI with their content instead of
    /// making the shared coordinator aware of each blade type.
    fn show_overlay(&mut self, _ctx: &egui::Context) {}

    fn resource_detail(&self) -> Option<&ResourceDetailHistoryEntry> {
        None
    }

    fn resource_detail_mut(&mut self) -> Option<&mut ResourceDetailHistoryEntry> {
        None
    }

    fn is_owned_by_resource_detail(&self, _history_entry_id: u64) -> bool {
        false
    }

    #[cfg(test)]
    fn terminal_settings(&self) -> Option<&super::settings::TerminalSettingsBlade> {
        None
    }
}

pub(super) struct GlobalBladeRenderContext<'a> {
    ui_state: &'a UiState,
    debug_image_presets: &'a [DebugImagePreset],
    table_preferences: &'a mut PersistedResourceTablePreferences,
    terminal_launch_settings: &'a mut TerminalLaunchSettings,
    update_status: &'a UpdateStatus,
}

impl<'a> GlobalBladeRenderContext<'a> {
    fn new(
        ui_state: &'a UiState,
        debug_image_presets: &'a [DebugImagePreset],
        table_preferences: &'a mut PersistedResourceTablePreferences,
        terminal_launch_settings: &'a mut TerminalLaunchSettings,
        update_status: &'a UpdateStatus,
    ) -> Self {
        Self {
            ui_state,
            debug_image_presets,
            table_preferences,
            terminal_launch_settings,
            update_status,
        }
    }

    pub(super) fn resource_navigation(
        &self,
        cluster_key: i32,
    ) -> crate::resource_catalog::ResourceNavigation {
        self.ui_state
            .clusters
            .get(&cluster_key)
            .map(|cluster| cluster.resource_navigation.clone())
            .unwrap_or_default()
    }

    pub(super) fn helm_releases(
        &self,
        cluster_key: i32,
        namespace: &str,
        release_name: &str,
    ) -> Vec<crate::helm_release::HelmRelease> {
        self.ui_state
            .helm_releases(cluster_key, namespace, release_name)
    }

    /// Resolves a manifest inventory entry against an already-synchronized resource watch.
    ///
    /// Inventory data is historical, so the inspector only enables its detail link when the
    /// object currently represented by the resource cache supplies a real UID.
    pub(super) fn cached_resource_uid(
        &self,
        cluster_key: i32,
        api_resource: &crate::api_resource::ApiResource,
        namespace: Option<&str>,
        name: &str,
    ) -> Option<String> {
        let watch = self
            .ui_state
            .clusters
            .get(&cluster_key)?
            .resource_cache
            .get(&(api_resource.clone(), namespace.map(ToOwned::to_owned)))?;
        if !watch.is_synced || watch.error.is_some() {
            return None;
        }
        watch
            .resources
            .values()
            .find(|resource| resource.name == name)
            .map(|resource| resource.uid.clone())
    }

    pub(super) fn supports_scale(
        &self,
        cluster_key: i32,
        api_resource: &crate::api_resource::ApiResource,
    ) -> bool {
        self.ui_state
            .clusters
            .get(&cluster_key)
            .is_some_and(|cluster| cluster.scalable_api_resources.contains(api_resource))
    }

    pub(super) fn debug_image_presets(&self) -> &[DebugImagePreset] {
        self.debug_image_presets
    }

    pub(super) fn table_preferences(&mut self) -> &mut PersistedResourceTablePreferences {
        self.table_preferences
    }

    pub(super) fn terminal_launch_settings(&mut self) -> &mut TerminalLaunchSettings {
        self.terminal_launch_settings
    }

    pub(super) fn update_status(&self) -> &UpdateStatus {
        self.update_status
    }
}

#[derive(Default)]
pub(super) struct GlobalBladeRenderResult {
    pub(super) close: bool,
    pub(super) next_content: Option<Box<dyn GlobalBladeContent>>,
}

/// A deferred operation originating from a specific blade content type.
/// Keeping this object dynamic prevents the shared coordinator from becoming
/// a growing action enum dispatcher.
pub(super) trait GlobalBladeEffect: std::fmt::Debug {
    fn apply(
        self: Box<Self>,
        context: &mut GlobalBladeEffectContext<'_>,
        navigation: &mut GlobalBladeNavigation<'_>,
    );
}

/// Operational services are intentionally separate from render services:
/// content can query only narrow render data, then request an effect after the
/// frame has finished borrowing the navigator.
pub(super) struct GlobalBladeEffectContext<'a> {
    pub(super) ctx: &'a egui::Context,
    pub(super) ui_state: &'a mut UiState,
    pub(super) shell_requests: &'a mut Vec<ShellRequest>,
}

/// The only navigation capability available to content effects. It keeps
/// history mutation and discarded-entry cleanup at the global boundary.
pub(super) struct GlobalBladeNavigation<'a> {
    navigator: &'a mut BladeNavigator<Box<dyn GlobalBladeContent>>,
    commands_to_send: &'a mut Vec<WorkerCommandBox>,
}

impl GlobalBladeNavigation<'_> {
    pub(super) fn push(&mut self, content: Box<dyn GlobalBladeContent>) {
        UiState::stop_discarded_blades(self.navigator.push(content), self.commands_to_send);
    }

    pub(super) fn current_mut(&mut self) -> &mut dyn GlobalBladeContent {
        self.navigator.current_mut().as_mut()
    }

    pub(super) fn commands_to_send(&mut self) -> &mut Vec<WorkerCommandBox> {
        self.commands_to_send
    }
}
