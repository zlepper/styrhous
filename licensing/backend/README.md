# Licensing backend

The backend solution contains two projects:

- `src/Styrhous.Licensing` is the single deployable executable. Runtime concerns are separated by
  folders and namespaces rather than assemblies.
- `tests/Styrhous.Licensing.Tests` contains NUnit unit, PostgreSQL integration, and API tests.

The executable exposes API, worker, one-shot maintenance, and migration command modes. The
committed EF migration is the production schema authority; API and worker startup fail when the
database has pending migrations. Pulumi remains separate under the licensing infrastructure
subtree because it is a deployment program rather than part of the runtime.

## Test the current slice

From the repository root, start the real PostgreSQL fixture:

```bash
docker compose --file licensing/compose.yaml up --detach postgres rabbitmq
```

The test fixture waits up to 30 seconds for PostgreSQL readiness. Docker Compose users may add
`--wait`; the command above also works with the NixOS Podman Compose wrapper, which lacks that flag.

Then restore the committed package graph and run the tests:

```bash
cd licensing/backend
dotnet restore Styrhous.Licensing.slnx --locked-mode
dotnet test Styrhous.Licensing.slnx --no-restore
```

The SDK is pinned in `global.json`. EF tooling is repository-local; from this directory, run
`dotnet tool restore` once, then use `dotnet ef` (no global tool installation is needed).

Each persistence test creates and drops its own database. Override the local connection when needed
with `STYRHOUS_LICENSING_TEST_POSTGRES`; otherwise tests use the Compose service on port `55432`.
Rebus integration tests use the Compose RabbitMQ service on port `55672`; override it with
`STYRHOUS_LICENSING_TEST_RABBITMQ`.

Application persistence is expressed through `LicensingDbContext`, LINQ, tracked entities,
transactions, bulk EF mutations, and EF optimistic concurrency. The migration command applies
schema changes with the database owner credential. Infrastructure provisions the restricted runtime
login in a separate task after migration. The test harness uses Npgsql directly for isolated
database/role lifecycle and permission verification.

Running the API or worker also requires `DataProtection__Certificate` containing a base64-encoded PKCS#12
certificate with a private key and, when applicable, `DataProtection__CertificatePassword`. The
key ring is stored in PostgreSQL and encrypted by that certificate; every API and worker process in
a deployment must use the same certificate and `Styrhous.Licensing` application name. Maintenance
only reads and publishes durable work identifiers, so it does not load protected payloads or need
the certificate. The certificate must contain an RSA public/private key pair. During rotation,
configure the new active certificate normally and retain each old decrypt-only certificate under
`DataProtection__PreviousCertificates__N__Certificate` and
`DataProtection__PreviousCertificates__N__CertificatePassword`. Do not remove an old certificate
while any persisted key-ring row encrypted by it, or any protected payload using that key, remains.
Tests generate isolated certificates automatically.

The API additionally requires the canonical public origin in
`DesktopProtocol__Issuer` (for example, `https://licenses.example.com/`), plus
`DesktopProtocol__Certificate` and optional
`DesktopProtocol__CertificatePassword` for OpenIddict desktop-token signing and encryption. Keep
this certificate separate from the Data Protection certificate: it must allow both digital
signatures and key encipherment, while the Data Protection certificate remains valid when it is
encryption-only. During desktop-protocol certificate rotation, retain prior certificates under
`DesktopProtocol__PreviousCertificates__N__Certificate` and the matching password key until every
device code, access token, and refresh token they protect has expired or been revoked. The current
desktop certificate must expire after every retained previous certificate; OpenIddict uses
certificate expiry when choosing the credential for new protocol state, and startup rejects a ring
whose current certificate is not yet valid, has expired, or could keep issuing with an old key. API
request logs deliberately record only the HTTP method, path, response status, and duration, never
query strings containing short-lived desktop user codes.

Browser accounts require at least one complete external authentication provider in production.
ASP.NET Core cookie authentication owns the application session, while the licensing account
aggregate retains external identities and verified provider history for trial and linking rules.
Configure a supported provider with both values from one of these pairs:

```text
Authentication__GitHub__ClientId / Authentication__GitHub__ClientSecret
Authentication__Google__ClientId / Authentication__Google__ClientSecret
Authentication__Microsoft__ClientId / Authentication__Microsoft__ClientSecret
```

Provider callback URLs use the canonical issuer host and the
`/auth/provider-callback/{github|google|microsoft}` paths. A provider is enabled only when both its
client ID and secret are present; partial configuration is rejected at startup.

Microsoft uses authorization-code OpenID Connect with PKCE. Enable work/school and personal
Microsoft accounts in the app registration and request the ID-token optional claims `email`,
`verified_primary_email`, `verified_secondary_email`, and `xms_edov`. Sign-in requires an
authoritative primary/secondary email, or `email` with boolean `xms_edov: true`. Plain email,
Graph `mail`, and UPN are not verification evidence. Accounts without these claims must use
another configured provider; Styrhous does not run its own email-verification flow. See Microsoft's
[optional claims reference](https://learn.microsoft.com/en-us/entra/identity-platform/optional-claims-reference).
The Graph `User.Read` delegated permission remains required to preserve existing login identifiers.

Stripe-backed billing additionally requires `Stripe__SecretKey` for authoritative worker reads and
`Stripe__WebhookSecret` for webhook signature verification. Configure the only entitlement-bearing
prices as `Stripe__MonthlyPriceId` and `Stripe__AnnualPriceId`; all other Stripe prices fail closed.
Initial Checkout also requires trusted HTTPS destinations in `Stripe__CheckoutSuccessUrl` and
`Stripe__CheckoutCancelUrl`. Keep the keys in the deployment's secret store; neither value is
written to logs or persisted in the licensing database.
Customer Portal redirects require a restricted Stripe configuration in
`Stripe__CustomerPortalConfigurationId` and the trusted HTTPS destination
`Stripe__CustomerPortalReturnUrl`. Disable subscription-quantity changes in that Stripe
configuration; Styrhous owns seat-capacity validation and changes quantities through its own API.
The API retrieves this configuration before every session and refuses inactive configurations or
any configuration with subscription updates enabled.

Organization Owners change paid capacity through
`PATCH /api/billing-accounts/{billingAccountId}/seat-quantity`. Increases are prorated and invoiced
immediately; decreases create no current-period credit and lower the next renewal. Each change uses
a durable UUIDv7 billing operation and stable operation-derived idempotency key. An ambiguous
provider failure returns that operation ID for an exact retry. Every attempt first retrieves and
projects authoritative Stripe state ordered by Stripe response time, mutation-response priority,
and a database-backed read revision. The exact observation is checked again under the
billing-account lock immediately around the provider mutation. If the prior update has not reached
Stripe, an exact retry may replay the identical update with the same idempotency key for up to 23
hours measured by Stripe's response clock; the final locked Stripe GET enforces that cutoff before
any POST. After that window the operation remains pending for safe observation and reconciliation,
and Styrhous will not issue a potentially duplicating key.
Current Owners can recover after ownership or subscription-state changes, while a pending decrease
already reserves the lower capacity against new seat-bearing invitations or product-seat
assignments. Organization roles and membership no longer consume capacity by themselves; only
enabled product seats and seat-bearing invitations do. Quantities have no product-level maximum
beyond the positive `int` representation and paid plans remain at least one seat.

Invitation delivery and supported Stripe webhook transactions write their work identifiers to
Rebus's PostgreSQL outbox in the same database transaction as the business change. Rebus forwards
committed messages to RabbitMQ for local and self-hosted deployments, or Amazon SQS for SaaS.
Broker outages leave those messages durable without delaying the HTTP request. Publication is at
least once: forwarding can repeat if a process stops after broker acceptance but before its outbox
transaction commits. Consumers use durable processing leases and completion state to handle retries.
Maintenance also backfills unfinished work created before the native outbox migration, once per
business record, and waits up to 45 seconds for native forwarding to drain. A timeout fails the
maintenance invocation while retaining pending messages for its next run.

Invitation delivery processing is also transport-neutral. A worker claim decrypts the protected
payload, serializes through the organization root, and revalidates the current invitation, secret,
recipient, role, and expiry in an EF transaction before granting a five-minute UUIDv7 processing
lease capped by the invitation deadline. Claim time is sampled after serialization, and the worker
rechecks the deadline immediately before and after the email-provider call. Successful in-window
email submission completes the durable outbox row; failures release the lease, and expired leases
may be reclaimed. Unreadable, invalid, or mismatched protected envelopes are durably quarantined so
maintenance cannot repeatedly wake the worker for poison work. Cancellation, acceptance, or resend
can discard a pending
delivery even after its message was published, invalidating any outstanding lease. Delivery is
intentionally at least once: if the email provider accepts a request but completion bookkeeping
fails, a retry may submit the same durable delivery identifier again. The eventual email adapter
must pass that identifier through wherever its provider supports deduplication.

## Runtime modes

API is the default, so existing ASP.NET command-line arguments continue to work:

```bash
dotnet run --project src/Styrhous.Licensing -- api
dotnet run --project src/Styrhous.Licensing -- worker
dotnet run --project src/Styrhous.Licensing -- maintenance
dotnet run --project src/Styrhous.Licensing -- migrate
```

Production deployment runs the infrastructure-owned database provisioning task after migrations.
It creates or updates the runtime login, revokes public schema creation, and grants the table and
sequence access needed by API, worker, and maintenance processes. These runtimes use the restricted
credential. See [infrastructure setup](../infrastructure/README.md).

`maintenance-lambda` hosts the same recovery operation behind a private HTTP
endpoint for the AWS Lambda Web Adapter. It is selected by the maintenance
function's container command and is not routed through the public API Gateway.

At process startup, production may load its mode-specific JSON configuration
from AWS Secrets Manager by setting `LICENSING_SECRET_ARN` and,
preferably, the immutable `LICENSING_SECRET_VERSION`. The version cannot be set
without the ARN. `STYRHOUS_APPLICATION_CONFIGURATION` remains available for local
or self-hosted inline JSON, but the inline and Secrets Manager sources are mutually
exclusive so their precedence can never be ambiguous. Ordinary environment and
command-line values remain available for non-secret settings; the loaded JSON has
final precedence.

The API runs Rebus's native outbox forwarder, `worker` runs the receiver until shutdown, and
`maintenance` starts the forwarder and exits after draining pending messages. A duplicate delivery
that encounters an active processing lease remains unacknowledged while the handler waits for
completion or lease expiry. Shutdown cancellation leaves it available for broker redelivery.

The API, worker, and both maintenance modes require the shared queue settings by default:

```text
Messaging__Transport=RabbitMq|AmazonSqs
Messaging__QueueName=styrhous-licensing
Messaging__ErrorQueueName=styrhous-licensing-error
Messaging__MaximumParallelism=4
```

Set `Messaging__ApiOutboxForwardingEnabled=false` only when deliberately forwarding through
maintenance alone. Invitation and webhook transactions still require `Messaging__QueueName` to
persist their destination, but the API does not open a broker connection.

RabbitMQ additionally requires `Messaging__RabbitMq__ConnectionString`. Amazon SQS requires
`Messaging__AmazonSqs__Region`; it uses the standard AWS role/credential chain and assumes queues
are provisioned unless `Messaging__AmazonSqs__CreateQueues=true` is explicitly selected.
Queue and error-queue names must differ and contain at most 80 letters, digits, hyphens, or
underscores. RabbitMQ sender-only mode also expects the durable destination queue to have been
declared by deployment or by starting the receiver once; it remains available while the worker is
scaled to zero.

The worker sends invitation email through SES v2 and requires:

```text
InvitationEmail__FromAddress=noreply@example.com
InvitationEmail__AcceptanceUrl=https://licenses.example.com/invitations/accept
InvitationEmail__AmazonSes__Region=eu-west-1
```

The sender URL must use HTTPS and must not already contain a query or fragment. The worker adds the protected
invitation secret as an encoded query parameter and tags the SES request with the non-secret
durable outbox UUID. Email addresses, bodies, and invitation secrets are not logged.
