use kube::Resource;

pub(crate) trait ResourceExt {
    fn try_get_display_name(&self) -> Option<String>;
}

impl<K> ResourceExt for K
where
    K: Resource,
{
    fn try_get_display_name(&self) -> Option<String> {
        let metadata = self.meta();

        if let Some(annotations) = &metadata.annotations {
            if let Some(display_name) = annotations.get("tesseract.dev/display-name") {
                return Some(display_name.clone());
            }
        }

        None
    }
}
