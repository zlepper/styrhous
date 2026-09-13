# Styrhous licensing system

## Product contract

Styrhous licensing is an honor-based commercial signal. An expired or unlicensed desktop
installation enters evaluation status and shows a warning; the backend does not remotely disable
desktop features.

- Licenses are recurring per-seat subscriptions. Personal users receive one personal seat;
  organizations purchase seats for product-enabled members.
- A verified first signup starts a 30-day trial without a card. The trial is retained across
  provider changes and may be transferred once to the user's first organization.
- Stripe hosts checkout, billing details, invoices, payment methods, cancellation, tax, and
  payment notifications. The backend owns seat-capacity validation and changes.
- Organizations have Owner, Admin, and Member roles. Role administration and product entitlement
  are separate: an administrator does not need a product seat.
- A seat has a stable UUIDv7 identity and defaults to three active desktop devices.
- Invitations reserve capacity only when they assign a product seat. They can be created,
  cancelled, resent, accepted, and delivered at least once.
- SaaS supports GitHub, Google, and Microsoft social login. A future self-hosted installation can
  use generic OIDC and a vendor-signed annual seat license.

The stable desktop interface is `/desktop/v1`; its OpenAPI document is embedded at
`backend/src/Styrhous.Licensing/Api/Desktop/desktop-v1-openapi.json`. It supports browser-approved
device authorization, tokens, seat selection, entitlement checks, signing-key discovery, and
signed offline leases. It is a compatibility boundary independent of the hosting platform.

## Monolith architecture

The deployed unit is one ASP.NET Core application with PostgreSQL:

```text
Browser portal + desktop clients
              |
      Fly public ASP.NET Core monolith
       |       |       |        |
    portal   API    Rebus      SMTP
       |       |       |        |
       +------- PostgreSQL ---+
```

The static SvelteKit portal is built into the web root and served from the same origin as the API.
Browser mutations use normal antiforgery protection and first-party secure cookies. API, auth, and
desktop responses are non-cacheable; the host sets browser security headers and does not route
unknown backend paths into the SPA.

EF Core owns the product schema and PostgreSQL persistence. Every Styrhous-owned identity is UUIDv7
and timestamps are UTC `DateTimeOffset` values. The web process validates that migrations are
current but never applies them. `migrate` is the release command that applies the committed EF
migrations before a deployment.

Rebus.PostgreSql stores queue and error-queue messages in the same PostgreSQL service. The durable
application outbox is still authoritative: an API request publishes a queue hint after its
transaction commits, and startup recovery republishes pending or stale work. Rebus retries a
message up to five times before moving it to the configured error queue. Invitation delivery,
Stripe webhook projection, and infrastructure probes run in the monolith's one receiver.

This low-cost phase deliberately has no independent scheduler or worker. A stopped application is
woken by the next browser, desktop, Stripe, or OAuth request; only then do durable recovery and
queued retries resume. A production reliability upgrade can add a minimum running machine or a
separate worker without changing the desktop contract or outbox semantics.

Invitation email uses SendGrid SMTP through MailKit. The protected invitation secret appears only
in the invitation URL; the durable delivery UUID is sent as a non-secret SMTP header for provider
deduplication. Credentials, recipient addresses, URLs, and email content are never logged.

## Deployment and recovery

The initial deployment is one public Fly machine and one private unmanaged Fly Postgres
Development machine, both in Amsterdam. The application machine uses Fly `stop` autostop and
autostart; it must not use suspend because Rebus polling connections could remain open. PostgreSQL
is reachable only through its Flycast private hostname and can scale to zero after Fly's
one-hour no-session development threshold. The connection pool has no minimum size and short idle
pruning so the stopped application leaves no database sessions behind.

There is no AWS, NAT gateway, Lambda, SQS, RabbitMQ, Pulumi stack, or public database endpoint.
Daily Fly volume snapshots are the selected early-stage recovery measure. A one-machine unmanaged
database is not highly available and snapshots are not a substitute for tested off-platform
backups.

See [backend runtime details](backend/README.md) and the [Fly bootstrap guide](fly/README.md).

## Validation

Backend tests use real PostgreSQL and cover migrations, EF persistence, API behavior, desktop
contracts, PostgreSQL Rebus send/receive, durable outbox recovery behavior, and SMTP message
conversion. The portal retains Svelte type, unit, browser, responsive, and accessibility coverage.

```bash
docker compose --file licensing/compose.yaml up --detach postgres
(cd licensing/backend && bash check.sh)
(cd licensing/portal && npm test)
```
