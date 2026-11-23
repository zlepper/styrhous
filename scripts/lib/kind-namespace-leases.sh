# shellcheck shell=bash
# Namespace leases of the Kind integration tests. Sourced by scripts/ensure-kind-cluster.sh and by
# the lease tests in crates/styrhous/src/ui/tests/kind/support/locks.rs.
#
# A running test holds an exclusive flock on `<lock dir>/styrhous-kind-namespaces/<namespace>.lock`
# from before it creates its namespace until after it has deleted it (`NamespaceLease`). Only
# namespaces whose lease can be taken are leftovers; the others belong to a concurrent test run.
# The kernel drops the lock of a crashed or interrupted test, so its namespace counts as leftover.

# Prints the directory of the Kind test locks. It derives from the user id rather than
# XDG_RUNTIME_DIR or TMPDIR, which differ between shells and services, so that every run on this
# machine sees the same leases. Must match `kind_lock_dir` in locks.rs (a test checks this).
kind_lock_dir() {
    local uid
    uid="$(id -u)"
    if [[ -d "/run/user/${uid}" ]]; then
        printf '%s\n' "/run/user/${uid}"
    else
        printf '%s\n' "/tmp/styrhous-kind-${uid}"
    fi
}

# take_leftover_namespace_leases LEASE_DIR NAMESPACE...
# Takes the lease of every given namespace that no running test owns and sets the arrays
# `test_namespaces` (the leftovers) and `lease_files` (their lease files). The taken leases stay
# locked until the calling shell exits, so a concurrent cleanup skips them while they are deleted.
take_leftover_namespace_leases() {
    local lease_dir="$1"
    shift
    mkdir -p "${lease_dir}"
    test_namespaces=()
    lease_files=()
    local namespace lease_file lease_fd
    for namespace in "$@"; do
        lease_file="${lease_dir}/${namespace}.lock"
        exec {lease_fd}<>"${lease_file}"
        if flock --nonblock "${lease_fd}"; then
            test_namespaces+=("${namespace}")
            lease_files+=("${lease_file}")
        else
            exec {lease_fd}>&-
            echo "Keeping test namespace '${namespace}': a running test or another cleanup owns it."
        fi
    done
}
