#!/usr/bin/env bash

set -euo pipefail

cluster_name="kind"
context_name="kind-${cluster_name}"
timeout="120s"
timeout_seconds=120
test_namespace_prefix="kdui-it-"

clusters="$(kind get clusters)"
if ! grep -Fxq "${cluster_name}" <<<"${clusters}"; then
    echo "Creating Kind cluster '${cluster_name}'..."
    kind create cluster --name "${cluster_name}" --wait "${timeout}"
else
    echo "Reusing Kind cluster '${cluster_name}'..."
    mapfile -t nodes < <(kind get nodes --name "${cluster_name}")
    if (( ${#nodes[@]} == 0 )); then
        echo "Kind cluster '${cluster_name}' has no node containers." >&2
        exit 1
    fi

    echo "Starting Kind node containers..."
    docker start "${nodes[@]}" >/dev/null
    kind export kubeconfig --name "${cluster_name}"
fi

echo "Waiting for Kind cluster '${cluster_name}' to be ready..."
deadline=$((SECONDS + timeout_seconds))
until kubectl --context "${context_name}" get --raw=/readyz >/dev/null 2>&1; do
    if (( SECONDS >= deadline )); then
        echo "Timed out waiting for the Kubernetes API server to become ready." >&2
        kubectl --context "${context_name}" cluster-info >&2 || true
        exit 1
    fi
    sleep 2
done

kubectl --context "${context_name}" wait --for=condition=Ready nodes --all --timeout="${timeout}"
kubectl --context "${context_name}" --namespace kube-system rollout status deployment/coredns --timeout="${timeout}"

if ! namespace_names="$(kubectl --context "${context_name}" get namespaces \
    --output=jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}')"; then
    echo "Failed to list namespaces before integration test cleanup." >&2
    exit 1
fi
mapfile -t test_namespaces < <(
    awk -v prefix="${test_namespace_prefix}" 'index($0, prefix) == 1' <<<"${namespace_names}"
)

if (( ${#test_namespaces[@]} > 0 )); then
    echo "Removing leftover Kubernetes Dev UI integration test namespaces..."
    namespace_resources=()
    for namespace in "${test_namespaces[@]}"; do
        namespace_resources+=("namespace/${namespace}")
        # The force-delete integration test is the only suite test that deliberately
        # adds finalizers, and it adds them to a ConfigMap. Clear them so an
        # interrupted test run cannot leave this namespace stuck in Terminating.
        if ! configmaps="$(kubectl --context "${context_name}" --namespace "${namespace}" \
            get configmaps --output=name)"; then
            echo "Failed to list ConfigMaps in leftover test namespace '${namespace}'." >&2
            exit 1
        fi
        if [[ -n "${configmaps}" ]]; then
            mapfile -t configmap_resources <<<"${configmaps}"
            for configmap in "${configmap_resources[@]}"; do
                kubectl --context "${context_name}" --namespace "${namespace}" \
                    patch "${configmap}" --type=merge --patch '{"metadata":{"finalizers":[]}}'
            done
        fi
    done
    kubectl --context "${context_name}" delete namespace "${test_namespaces[@]}" \
        --ignore-not-found --wait=false
    kubectl --context "${context_name}" wait --for=delete "${namespace_resources[@]}" \
        --timeout="${timeout}"
fi
