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
