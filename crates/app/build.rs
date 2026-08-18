fn main() {
    println!("cargo::rustc-check-cfg=cfg(kubernetes_dev_ui_release_build)");
    println!("cargo::rustc-check-cfg=cfg(kubernetes_dev_ui_package_managed_build)");
    println!("cargo::rerun-if-env-changed=KUBERNETES_DEV_UI_RELEASE_BUILD");
    println!("cargo::rerun-if-env-changed=KUBERNETES_DEV_UI_DISABLE_AUTO_UPDATE");
    println!("cargo::rerun-if-env-changed=KUBERNETES_DEV_UI_UPDATER_PUBLIC_KEY");

    if std::env::var("KUBERNETES_DEV_UI_RELEASE_BUILD").as_deref() == Ok("1") {
        println!("cargo::rustc-cfg=kubernetes_dev_ui_release_build");
    }
    if std::env::var("KUBERNETES_DEV_UI_DISABLE_AUTO_UPDATE").is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        )
    }) {
        println!("cargo::rustc-cfg=kubernetes_dev_ui_package_managed_build");
    }
}
