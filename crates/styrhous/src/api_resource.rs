#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ApiResource {
    pub group: String,
    pub version: String,
    pub kind: String,
    pub name: String,
    pub namespaced: bool,
}

impl ApiResource {
    pub(crate) fn helm_releases() -> Self {
        Self {
            group: crate::helm_release::GROUP.to_owned(),
            version: crate::helm_release::VERSION.to_owned(),
            kind: crate::helm_release::KIND.to_owned(),
            name: crate::helm_release::NAME.to_owned(),
            namespaced: true,
        }
    }

    pub(crate) fn is_helm_releases(&self) -> bool {
        self == &Self::helm_releases()
    }

    /// A human-readable, plural label for use in resource navigation.
    ///
    /// Kubernetes discovery gives resource names as lowercase plurals, but its Kind retains
    /// the word boundaries and casing needed for a readable label.
    pub fn display_name(&self) -> String {
        let mut display_name = split_kind_words(&self.kind);

        match plural_suffix(&self.kind, &self.name) {
            Some("ies") => {
                display_name.pop();
                display_name.push_str("ies");
            }
            Some(suffix) => display_name.push_str(suffix),
            None => {}
        }

        display_name
    }
}

fn split_kind_words(kind: &str) -> String {
    let mut display_name = String::with_capacity(kind.len() + 4);
    let mut characters = kind.chars().peekable();
    let mut previous = None;

    while let Some(character) = characters.next() {
        let next = characters.peek().copied();
        if character.is_uppercase()
            && previous.is_some_and(|previous: char| {
                previous.is_lowercase()
                    || (previous.is_uppercase() && next.is_some_and(char::is_lowercase))
            })
        {
            display_name.push(' ');
        }
        if display_name.is_empty() {
            display_name.extend(character.to_uppercase());
        } else {
            display_name.push(character);
        }
        previous = Some(character);
    }

    display_name
}

fn plural_suffix(kind: &str, resource_name: &str) -> Option<&'static str> {
    let kind = kind.to_ascii_lowercase();
    let resource_name = resource_name.to_ascii_lowercase();

    if resource_name == kind {
        None
    } else if resource_name == format!("{kind}s") {
        Some("s")
    } else if resource_name == format!("{kind}es") {
        Some("es")
    } else if let Some(stem) = kind.strip_suffix('y')
        && resource_name == format!("{stem}ies")
    {
        Some("ies")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(kind: &str, name: &str) -> ApiResource {
        ApiResource {
            group: "example.dev".into(),
            version: "v1".into(),
            kind: kind.into(),
            name: name.into(),
            namespaced: true,
        }
    }

    #[test]
    fn display_name_uses_kind_word_boundaries_and_resource_plural() {
        assert_eq!(
            resource("HorizontalPodAutoscaler", "horizontalpodautoscalers").display_name(),
            "Horizontal Pod Autoscalers"
        );
    }

    #[test]
    fn display_name_preserves_acronyms_and_common_plural_suffixes() {
        assert_eq!(
            resource("HTTPRoute", "httproutes").display_name(),
            "HTTP Routes"
        );
        assert_eq!(
            resource("PriorityClass", "priorityclasses").display_name(),
            "Priority Classes"
        );
        assert_eq!(
            resource("NetworkPolicy", "networkpolicies").display_name(),
            "Network Policies"
        );
    }
}
