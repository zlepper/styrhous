# Contributing to Styrhous

Styrhous is source available, not open source. The repository license permits
local nonproduction evaluation and the work needed to prepare a contribution;
it does not permit production use, commercial exploitation, or general
redistribution. Read [LICENSE.md](LICENSE.md) before using the repository.

## Contributor agreement

Every human contributor must accept the
[Styrhous Contributor License Agreement](CLA.md) before a pull request can be
merged. The CLA lets you retain copyright in your contribution while giving
Rasmus Hansen broad, perpetual, irrevocable, transferable, and sublicensable
rights to use, modify, commercialize, and relicense it without asking for
further permission or paying compensation.

CLA Assistant records acceptance and the signer's declared capacity. Sign in
your individual capacity when you own the work personally. If an employer,
client, or other entity owns or may own it, select the entity capacity, identify
the entity, and confirm that you have authority to bind it. If you do not have
that authority, ask an authorized representative to accept through CLA
Assistant before the contribution is merged.

Do not submit third-party code, generated material, images, fonts, designs, or
other assets unless you identify their source and license in the pull request
and receive written approval first. Do not submit credentials, personal data,
or confidential information.

## Development workflow

- Follow the repository guidance in `AGENTS.md`.
- Keep Kubernetes operations behind the worker command/result boundary.
- Use existing components and shared design tokens for UI work.
- Add behavior-focused tests and use the project's paired pixel and
  accessibility snapshot harness for new snapshots.
- Run normal tests with nextest, never `cargo test`.
- Never bypass pre-commit hooks.

Submitting a contribution does not guarantee review, acceptance, attribution,
payment, or inclusion in a commercial release.

Commercial-license questions and legal notices should be sent to
`styrhous@zlepper.dk`.
