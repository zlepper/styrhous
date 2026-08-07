#!/usr/bin/env bash

set -euo pipefail

cluster_name="kind"
context_name="kind-${cluster_name}"
timeout="120s"
timeout_seconds=120

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
