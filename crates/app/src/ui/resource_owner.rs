use super::state::ResourceAction;
use crate::resource_catalog::ResourceNavigation;
use crate::resource_detail::ResourceOwner;

pub(super) fn navigation_action(
    navigation: &ResourceNavigation,
    owner: &ResourceOwner,
    subject_namespace: Option<&str>,
) -> Option<ResourceAction> {
    navigation
        .api_resource_for_owner(owner)
        .map(|api_resource| ResourceAction::NavigateDetails {
            namespace: api_resource
                .namespaced
                .then(|| subject_namespace.map(str::to_owned))
                .flatten(),
            api_resource,
            name: owner.name.clone(),
            uid: owner.uid.clone(),
        })
}

pub(super) fn queue_navigation_action(
    pending_action: &mut Option<ResourceAction>,
    action: ResourceAction,
) {
    if pending_action.is_none() {
        *pending_action = Some(action);
    }
}

pub(super) fn unavailable_tooltip(owner: &ResourceOwner) -> String {
    format!(
        "Details for {} are unavailable because this resource type is not available on the cluster.",
        owner.label()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_resource::ApiResource;
    use crate::resource_catalog::build_resource_navigation;

    fn owner(api_version: &str, kind: &str) -> ResourceOwner {
        ResourceOwner {
            api_version: api_version.into(),
            kind: kind.into(),
            name: "owner".into(),
            uid: "owner-uid".into(),
            controller: true,
        }
    }

    #[test]
    fn cluster_scoped_owners_navigate_without_a_namespace() {
        let node = ApiResource {
            group: "core".into(),
            version: "v1".into(),
            kind: "Node".into(),
            name: "nodes".into(),
            namespaced: false,
        };
        let navigation = build_resource_navigation(vec![node.clone()]);

        assert!(matches!(
            navigation_action(&navigation, &owner("v1", "Node"), Some("kube-system")),
            Some(ResourceAction::NavigateDetails {
                api_resource,
                namespace: None,
                name,
                uid,
            }) if api_resource == node && name == "owner" && uid == "owner-uid"
        ));
    }

    #[test]
    fn unresolved_owners_cannot_navigate_and_explain_why() {
        let owner = owner("example.dev/v1", "Widget");

        assert!(
            navigation_action(&ResourceNavigation::default(), &owner, Some("default")).is_none()
        );
        assert!(unavailable_tooltip(&owner).contains("not available on the cluster"));
    }
}
