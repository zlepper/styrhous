#!/usr/bin/env bash
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
hook="$repository_root/.githooks/pre-commit"
bash_executable="$(command -v bash)"
git_executable="$(command -v git)"
temporary_root="$(mktemp -d)"
test_repository="$temporary_root/repository"
stub_directory="$temporary_root/bin"
command_log="$temporary_root/commands.log"
empty_hooks="$temporary_root/empty-hooks"

while IFS= read -r git_environment_variable; do
  unset "$git_environment_variable"
done < <("$git_executable" rev-parse --local-env-vars)

cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT

mkdir -p \
  "$test_repository/.githooks" \
  "$test_repository/crates/styrhous/src" \
  "$test_repository/licensing/portal/src" \
  "$test_repository/licensing/protocol-fixtures" \
  "$test_repository/licensing/worker/src" \
  "$test_repository/scripts" \
  "$stub_directory" \
  "$empty_hooks"

cat >"$stub_directory/command-stub" <<'EOF'
#!/bin/sh
set -eu
printf '%s|%s|%s\n' "${0##*/}" "$PWD" "$*" >>"$PRE_COMMIT_TEST_LOG"
EOF
chmod +x "$stub_directory/command-stub"
for command_name in cargo npm; do
  ln -s command-stub "$stub_directory/$command_name"
done

cat >"$stub_directory/git" <<'EOF'
#!/bin/sh
exec "$PRE_COMMIT_TEST_REAL_GIT" "$@"
EOF
chmod +x "$stub_directory/git"

printf '[workspace]\n' >"$test_repository/Cargo.toml"
printf 'fn placeholder() {}\n' >"$test_repository/crates/styrhous/src/lib.rs"
printf 'export const existing = true;\n' >"$test_repository/licensing/portal/src/existing.ts"
printf 'export const existing = true;\n' >"$test_repository/licensing/worker/src/existing.ts"
printf '{}\n' >"$test_repository/licensing/protocol-fixtures/lease.json"
cp "$hook" "$test_repository/.githooks/pre-commit"
cat >"$test_repository/scripts/test-pre-commit.sh" <<'EOF'
#!/usr/bin/env bash
printf 'bash|%s|scripts/test-pre-commit.sh\n' "$PWD" >>"$PRE_COMMIT_TEST_LOG"
EOF
chmod +x "$test_repository/.githooks/pre-commit" \
  "$test_repository/scripts/test-pre-commit.sh"

git -C "$test_repository" init --quiet
git -C "$test_repository" add .
git -C "$test_repository" \
  -c commit.gpgsign=false \
  -c core.hooksPath="$empty_hooks" \
  -c user.email=pre-commit-test@styrhous.invalid \
  -c user.name="Pre-commit test" \
  commit --quiet --message "Initial fixtures"

export PATH="$stub_directory:$PATH"
export PRE_COMMIT_TEST_LOG="$command_log"
export PRE_COMMIT_TEST_REAL_GIT="$git_executable"

reset_scenario() {
  git -C "$test_repository" reset --hard --quiet HEAD
  git -C "$test_repository" clean -fdq
  : >"$command_log"
}

stage_change() {
  local path="$1"
  mkdir -p "$(dirname "$test_repository/$path")"
  printf 'changed\n' >"$test_repository/$path"
  git -C "$test_repository" add -- "$path"
}

run_hook() {
  (
    cd "$test_repository"
    "$bash_executable" .githooks/pre-commit
  )
}

assert_log() {
  local expected="$temporary_root/expected.log"
  : >"$expected"
  for line in "$@"; do
    printf '%s\n' "$line" >>"$expected"
  done
  diff -u "$expected" "$command_log"
}

reset_scenario
stage_change "licensing/worker/src/existing.ts"
run_hook
assert_log \
  "npm|$test_repository/licensing/worker|ci" \
  "npm|$test_repository/licensing/worker|run check" \
  "npm|$test_repository/licensing/worker|test"

reset_scenario
stage_change $'licensing/portal/src/new\nline.ts'
run_hook
assert_log \
  "npm|$test_repository/licensing/portal|ci" \
  "npm|$test_repository/licensing/portal|test"

reset_scenario
stage_change "crates/styrhous/src/lib.rs"
run_hook
assert_log \
  "cargo|$test_repository|fmt --check" \
  "cargo|$test_repository|clippy --workspace --all-targets --all-features -- -D warnings" \
  "cargo|$test_repository|nextest run --workspace"

reset_scenario
stage_change "licensing/protocol-fixtures/lease.json"
run_hook
assert_log \
  "cargo|$test_repository|fmt --check" \
  "cargo|$test_repository|clippy --workspace --all-targets --all-features -- -D warnings" \
  "cargo|$test_repository|nextest run --workspace" \
  "npm|$test_repository/licensing/worker|ci" \
  "npm|$test_repository/licensing/worker|run check" \
  "npm|$test_repository/licensing/worker|test"

reset_scenario
stage_change "licensing/README.md"
run_hook
assert_log
