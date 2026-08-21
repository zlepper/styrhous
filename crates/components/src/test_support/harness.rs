use super::*;

impl<State> UiHarnessSnapshot for Harness<'_, State> {
    fn try_ui_harness(
        &mut self,
        options: impl Into<HarnessSnapshotOptions>,
    ) -> Result<(), UiHarnessSnapshotError> {
        let options = options.into();
        let pixel = self.try_snapshot_options(&options.name, &options.pixel);
        let accessibility =
            self.try_accessibility_snapshot_with_options(&options.name, &options.accessibility);
        let nodes = current_accessibility_nodes(self);
        let labels = find_unlabeled_interactive_nodes(&nodes);
        let overlaps = if options.accessibility.check_illegal_overlaps {
            find_illegal_overlaps(&nodes)
        } else {
            Vec::new()
        };

        let error = UiHarnessSnapshotError {
            pixel: pixel.err().map(Box::new),
            accessibility: accessibility.err().map(Box::new),
            labels,
            overlaps,
        };
        if error.pixel.is_none()
            && error.accessibility.is_none()
            && error.labels.is_empty()
            && error.overlaps.is_empty()
        {
            Ok(())
        } else {
            Err(error)
        }
    }
}
