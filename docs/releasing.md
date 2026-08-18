# Releasing Kubernetes Dev UI

The release workflow runs when a `vX.Y.Z` tag is pushed. The tag must match the version in
`crates/app/Cargo.toml` exactly.

Before the first release, create the updater signing key once:

```bash
cargo packager signer generate
```

Store the printed private key and password as the GitHub Actions secrets
`CARGO_PACKAGER_SIGN_PRIVATE_KEY` and `CARGO_PACKAGER_SIGN_PRIVATE_KEY_PASSWORD` in the
protected `release` environment, not as repository-level secrets. Store the public key as the
repository variable `KUBERNETES_DEV_UI_UPDATER_PUBLIC_KEY`. Keep the private key backed up
securely: changing it prevents already-installed versions from trusting updates signed by the
replacement key.

Before enabling releases, configure the `release` environment in GitHub with required reviewers
and restrict deployments to protected `v*` tags. Environment secrets are only made available to
the direct package jobs after that approval. The workflow intentionally uses immutable action
commit IDs. After this workflow revision has been merged, enable **Require actions to be pinned
to a full-length commit SHA** in **Settings → Actions → General**, then select **Allow actions
and reusable workflows**. Allow GitHub-owned actions and whitelist only
`Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6`, while enforcing the full-SHA
pin requirement. Rust itself is pinned in `rust-toolchain.toml`; the release workflow pins
`cargo-nextest` and `cargo-packager` in its top-level environment.

A push to trusted `master` primes a dedicated, version-keyed cache for those Cargo extensions on
each release runner platform. GitHub intentionally isolates caches produced by distinct tags, but
release tags may restore caches produced by the default branch. Keeping the extensions separate
from the lockfile-keyed workspace build cache avoids recompiling them for every release while
still invalidating the cache when either pinned extension version changes.
Merge this workflow change to `master` and let its priming job complete before creating the next
release tag; a tag created first must install the tools once because it cannot read another tag's
cache. The same one-time compilation is expected whenever either pinned Cargo extension version
changes.

The workflow publishes installers to GitHub Releases and creates a per-platform update manifest.
Direct-download DMG, NSIS, and AppImage builds download and verify updates in the background,
then apply them on the next launch. Debian packages are built with automatic updates disabled.

Any package manager or administrator can opt out at launch with:

```bash
KUBERNETES_DEV_UI_DISABLE_AUTO_UPDATE=1 kubernetes-dev-ui
```

The opt-out accepts `1`, `true`, or `yes`, case-insensitively and with surrounding whitespace
ignored.

Preview releases are normal GitHub Releases with “Preview” in their title and notes. They must not
be marked as GitHub prereleases, because the updater intentionally uses GitHub’s stable
`releases/latest/download` endpoint.

Before stable public releases, configure macOS Developer ID signing/notarization and Windows
Authenticode signing in the workflow. The updater signature protects downloaded update payloads;
it does not replace the operating system’s installer trust requirements.

## Linux CI diagnostic image

`scripts/Dockerfile.ci` is a pinned Ubuntu 24.04 image for reproducing Linux CI test issues without
reinstalling the Rust toolchain, `cargo-nextest`, and Mesa dependencies each time:

```bash
docker build --file scripts/Dockerfile.ci --tag kubernetes-dev-ui-ci .
docker run --rm --env WGPU_BACKEND=gl \
  --volume "$PWD:/workspace" kubernetes-dev-ui-ci \
  cargo nextest run --workspace --no-fail-fast --test-threads 1 \
  -E 'not test(/kind_integration/)'
```

The Dockerfile compiles placeholder workspace targets after copying `Cargo.toml`, `Cargo.lock`,
and `rust-toolchain.toml`, then removes only the placeholder packages. The resulting dependency
build cache stays in the image at `CARGO_TARGET_DIR`; source files from the mounted checkout
rebuild the workspace crates without inheriting placeholder artifacts.
The Kind-backed integration tests require a host container runtime, so run those on the host or
in GitHub Actions rather than inside this diagnostic image.
The image runs as its unprivileged `kdui` user (UID/GID 1000), matching the normal development
container user and preventing root-owned files in the bind-mounted checkout.
