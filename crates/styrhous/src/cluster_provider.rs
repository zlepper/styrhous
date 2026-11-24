//! What a kubeconfig context says about where its cluster runs: the provider
//! and, when the context names it, the region, shown on the sidebar's cluster
//! card ("KIND · LOCAL", "EKS · EU-WEST-1").

/// The `server` URL's host, without scheme, port or path.
fn host_of(server: &str) -> &str {
    let rest = server.split_once("://").map_or(server, |(_, rest)| rest);
    let authority = rest.split(['/', '?']).next().unwrap_or(rest);
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    match authority.strip_prefix('[') {
        // A bracketed IPv6 literal.
        Some(literal) => literal.split(']').next().unwrap_or(literal),
        None => authority.split(':').next().unwrap_or(authority),
    }
}

fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback() || ip.is_unspecified())
}

/// Whether `region` looks like a cloud region such as `eu-west-1`.
fn is_region(region: &str) -> bool {
    let mut parts = region.split('-').collect::<Vec<_>>();
    let number = parts.pop().unwrap_or_default();
    parts.len() >= 2
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase()))
}

/// A short description of where the context's cluster runs, upper case as the
/// card shows it, or `None` when the provider is not recognisable.
///
/// Local clusters are recognised by the context name (`kind-*`, `k3d-*`,
/// `minikube`, `docker-desktop`, `rancher-desktop`); managed ones by the
/// server's host name (EKS, AKS) or the context name (`gke_project_zone_name`).
pub(crate) fn describe(context: &str, server: Option<&str>) -> Option<String> {
    let host = server.map(host_of).unwrap_or_default().to_lowercase();
    // A managed server host outranks whatever the context was named.
    if let Some(rest) = host.strip_suffix(".eks.amazonaws.com") {
        // <id>.<suffix>.<region>.eks.amazonaws.com
        return Some(provider_in(
            "EKS",
            rest.rsplit('.').next().filter(|region| is_region(region)),
        ));
    }
    if let Some(rest) = host.strip_suffix(".azmk8s.io") {
        // <name>-<id>.hcp.<region>.azmk8s.io
        return Some(provider_in(
            "AKS",
            rest.rsplit('.').next().filter(|region| *region != "hcp"),
        ));
    }
    if let Some(rest) = context.strip_prefix("gke_") {
        // gke_<project>_<zone or region>_<cluster>
        return Some(provider_in("GKE", rest.split('_').nth(1)));
    }
    let context = context.to_lowercase();
    let named = |name: &str| {
        context == name
            || context
                .strip_prefix(name)
                .is_some_and(|rest| rest.starts_with('-'))
    };
    let local = [
        ("kind", "KIND"),
        ("k3d", "K3D"),
        ("rancher-desktop", "RANCHER DESKTOP"),
        ("docker-desktop", "DOCKER DESKTOP"),
        ("docker-for-desktop", "DOCKER DESKTOP"),
        ("minikube", "MINIKUBE"),
    ];
    if let Some((_, provider)) = local.iter().find(|(name, _)| named(name)) {
        return Some(format!("{provider} · LOCAL"));
    }
    is_loopback(&host).then(|| "LOCAL".to_owned())
}

fn provider_in(provider: &str, region: Option<&str>) -> String {
    match region {
        Some(region) => format!("{provider} · {}", region.to_uppercase()),
        None => provider.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::describe;

    #[test]
    fn local_distributions_are_recognised_by_context_name() {
        for (context, expected) in [
            ("kind-kind", "KIND · LOCAL"),
            ("kind-legacy", "KIND · LOCAL"),
            ("kind-staging", "KIND · LOCAL"),
            ("k3d-dev", "K3D · LOCAL"),
            ("minikube", "MINIKUBE · LOCAL"),
            ("docker-desktop", "DOCKER DESKTOP · LOCAL"),
            ("rancher-desktop", "RANCHER DESKTOP · LOCAL"),
        ] {
            assert_eq!(
                describe(context, Some("https://127.0.0.1:6443")).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(describe(context, None).as_deref(), Some(expected));
        }
    }

    #[test]
    fn managed_clusters_are_recognised_by_their_server_host() {
        assert_eq!(
            describe(
                "arn:aws:eks:eu-west-1:1:cluster/prod",
                Some("https://ABC123.gr7.eu-west-1.eks.amazonaws.com")
            )
            .as_deref(),
            Some("EKS · EU-WEST-1")
        );
        assert_eq!(
            describe(
                "prod-aks",
                Some("https://prod-dns-1a2b.hcp.westeurope.azmk8s.io:443")
            )
            .as_deref(),
            Some("AKS · WESTEUROPE")
        );
        assert_eq!(
            describe("gke_proj_europe-west1-b_main", Some("https://34.1.2.3")).as_deref(),
            Some("GKE · EUROPE-WEST1-B")
        );
        assert_eq!(
            describe("prod", Some("https://x.eks.amazonaws.com")).as_deref(),
            Some("EKS")
        );
        assert_eq!(
            describe("prod", Some("https://my-cluster.eks.amazonaws.com")).as_deref(),
            Some("EKS")
        );
    }

    #[test]
    fn a_managed_host_outranks_the_context_name_and_names_need_a_boundary() {
        assert_eq!(
            describe(
                "kind-staging",
                Some("https://x.gr7.eu-west-1.eks.amazonaws.com")
            )
            .as_deref(),
            Some("EKS · EU-WEST-1")
        );
        assert_eq!(
            describe("minikube-shared", None).as_deref(),
            Some("MINIKUBE · LOCAL")
        );
        assert_eq!(describe("minikubeX", None), None);
        assert_eq!(describe("kindred", Some("https://10.0.0.1")), None);
        assert_eq!(describe("dev", Some("https://127.example.com")), None);
        assert_eq!(
            describe("prod", Some("https://my-cluster.eks.amazonaws.com")).as_deref(),
            Some("EKS")
        );
    }

    #[test]
    fn an_unknown_loopback_server_is_local_and_anything_else_is_omitted() {
        assert_eq!(
            describe("dev", Some("https://localhost:8443/k8s")).as_deref(),
            Some("LOCAL")
        );
        assert_eq!(
            describe("dev", Some("https://[::1]:6443")).as_deref(),
            Some("LOCAL")
        );
        assert_eq!(describe("prod-eu", Some("https://10.0.0.5:6443")), None);
        assert_eq!(describe("prod-eu", None), None);
        assert_eq!(describe("dev-eu", Some("https://api.example.com")), None);
    }
}
