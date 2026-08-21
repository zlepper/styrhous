use super::*;

#[test]
fn clicking_a_history_blade_returns_to_that_history_entry() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let replica_set = fixture_api_resource("apps", "ReplicaSet", "replicasets");
    let mut commands = Vec::new();
    harness.state_mut().ui_state.open_resource_detail(
        2,
        deployment.clone(),
        "api".into(),
        Some("kube-system".into()),
        "deployment-uid".into(),
        &mut commands,
    );
    harness.run_steps(2);
    harness.state_mut().ui_state.navigate_resource_detail(
        2,
        replica_set,
        "api-7b948f".into(),
        Some("kube-system".into()),
        "replicaset-uid".into(),
        &mut commands,
    );
    harness.run_steps(2);

    harness.get_by_label("Go back one blade").click();
    harness.run_steps(4);

    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_some()
    );
    assert_eq!(
        harness
            .state()
            .ui_state
            .global_blades
            .navigator()
            .unwrap()
            .current()
            .resource_detail()
            .unwrap()
            .api_resource,
        deployment
    );
}

#[test]
fn promoted_history_blade_stays_above_its_back_history() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let mut commands = Vec::new();

    harness.state_mut().ui_state.open_resource_detail(
        2,
        deployment.clone(),
        "first".into(),
        Some("kube-system".into()),
        "first-uid".into(),
        &mut commands,
    );
    harness.state_mut().worker.commands.append(&mut commands);
    harness.run_steps(1);
    harness
        .state_mut()
        .ui_state
        .global_blades
        .navigator_mut()
        .unwrap()
        .clear_transition();
    harness.run_steps(1);

    for (name, uid) in [("second", "second-uid"), ("third", "third-uid")] {
        harness.state_mut().ui_state.navigate_resource_detail(
            2,
            deployment.clone(),
            name.into(),
            Some("kube-system".into()),
            uid.into(),
            &mut commands,
        );
        harness.state_mut().worker.commands.append(&mut commands);
        harness
            .state_mut()
            .ui_state
            .global_blades
            .navigator_mut()
            .unwrap()
            .clear_transition();
        harness.run_steps(1);
    }

    for forward in [false, false, true] {
        harness
            .state_mut()
            .ui_state
            .navigate_resource_detail_history(2, forward, &mut commands);
        harness.state_mut().worker.commands.append(&mut commands);
        harness
            .state_mut()
            .ui_state
            .global_blades
            .navigator_mut()
            .unwrap()
            .clear_transition();
        harness.run_steps(1);
    }

    let navigator = harness.state().ui_state.global_blades.navigator().unwrap();
    assert_eq!(
        navigator.current().resource_detail().unwrap().resource_name,
        "second"
    );
    assert_eq!(navigator.back_stack().len(), 1);
    let active_layer = egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("global-blade-stack").with(("blade", navigator.back_stack().len())),
    );
    let active_header = harness.get_by_label("Back").rect().center();
    assert_eq!(harness.ctx.layer_id_at(active_header), Some(active_layer));
}
