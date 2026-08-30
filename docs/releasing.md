# Releasing Styrhous

The release workflow runs when a `vX.Y.Z` tag is pushed. The tag must match the version in
`crates/styrhous/Cargo.toml` exactly.

Before the first release, create the updater signing key once:

```bash
cargo packager signer generate
```

Store the printed private key and password as the GitHub Actions secrets
`CARGO_PACKAGER_SIGN_PRIVATE_KEY` and `CARGO_PACKAGER_SIGN_PRIVATE_KEY_PASSWORD` in the
protected `release` environment, not as repository-level secrets. Store the public key as the
repository variable `STYRHOUS_UPDATER_PUBLIC_KEY`. Keep the private key backed up
securely: changing it prevents already-installed versions from trusting updates signed by the
replacement key.

Before enabling releases, configure the `release` environment in GitHub with required reviewers
and restrict deployments to protected `v*` tags. Environment secrets are only made available to
the direct package jobs after that approval. The workflow intentionally uses immutable action
commit IDs. In **Settings → Actions → General**, require actions to be pinned to a full-length
commit SHA and allow GitHub-owned actions plus only
`Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6` and
`taiki-e/install-action@13608cbb45b01feb47ef444ab1a42dc41ad56f1a`. Update that allow-list
before pushing a workflow that introduces or changes an external action. Rust itself is pinned in
`rust-toolchain.toml`; the workflows pin their Cargo tool versions at the installation step.

Every branch push runs the Linux workspace checks. A push to trusted `master` also validates the
native package matrix and warms each platform's ordinary Rust build/tool cache. Release jobs use
the same job identities and cache configuration, so a tag can restore compatible default-branch
cache entries. Application executables are never cached between workflows: every release package
is rebuilt from the tagged source before signing.

The tag workflow deliberately trusts those prior checks instead of rerunning the workspace suite.
Before pushing a release tag, verify that both **Tests** and **Validate desktop packages** succeeded
for the exact commit being tagged. Release-environment reviewers should confirm the same commit
status before approving access to the updater signing key.

Third-party license policy, templates, and resource manifests live under `legal/`. Generated
license HTML and corresponding source archives are intentionally not tracked. To package locally,
install `cargo-about` 0.9.1 and run:

```bash
bash scripts/generate-third-party-legal.sh
```

The command writes the package inputs to ignored `target/legal/`. CI generates them once on Linux
and passes them to the native package jobs as short-lived workflow artifacts.

The workflow publishes installers to GitHub Releases and creates a per-platform update manifest.
Direct-download DMG, NSIS, and AppImage builds download and verify updates in the background,
then apply them on the next launch. Debian packages are built with automatic updates disabled.

Any package manager or administrator can opt out at launch with:

```bash
STYRHOUS_DISABLE_AUTO_UPDATE=1 styrhous
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
docker build --file scripts/Dockerfile.ci --tag styrhous-ci .
docker run --rm --env WGPU_BACKEND=gl \
  --volume "$PWD:/workspace" styrhous-ci \
  cargo nextest run --workspace --locked \
  -E 'not test(/ui::tests::kind::/)'
```

The Dockerfile compiles placeholder workspace targets after copying `Cargo.toml`, `Cargo.lock`,
and `rust-toolchain.toml`, then removes only the placeholder packages. The resulting dependency
build cache stays in the image at `CARGO_TARGET_DIR`; source files from the mounted checkout
rebuild the workspace crates without inheriting placeholder artifacts.
The Kind-backed integration tests require a host container runtime, so run those on the host or
in GitHub Actions rather than inside this diagnostic image.
The image runs as its unprivileged `styrhous` user (UID/GID 1000), matching the normal development
container user and preventing root-owned files in the bind-mounted checkout.
