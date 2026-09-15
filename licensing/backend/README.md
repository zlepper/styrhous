# Licensing backend

The licensing service is one ASP.NET Core monolith. It serves the versioned
`/desktop/v1` protocol, account and billing APIs, the static customer portal, Rebus handlers,
and startup recovery of the durable outbox. `migrate` is its only non-web command.

## Local validation

Start and retain PostgreSQL, then run the locked backend gate:

```bash
docker compose --file licensing/compose.yaml up --detach postgres
cd licensing/backend
bash check.sh
```

The tests create and drop isolated PostgreSQL databases. Set
`STYRHOUS_LICENSING_TEST_POSTGRES` to use another PostgreSQL instance. No RabbitMQ service is
needed.

## Runtime configuration

Configuration uses ordinary .NET environment variables, command-line values, and configuration
providers. Secrets are injected as environment variables; they are never read from an AWS secret
store. The production service requires:

```text
ConnectionStrings__Licensing=Host=styrhous-licensing-db.flycast;Port=5432;Database=postgres;Username=...;Password=...
Messaging__QueueName=styrhous-licensing
Messaging__ErrorQueueName=styrhous-licensing-error
InvitationEmail__FromAddress=noreply@example.com
InvitationEmail__AcceptanceUrl=https://licenses.example.com/invitations/accept
InvitationEmail__Smtp__Host=smtp.sendgrid.net
InvitationEmail__Smtp__Port=587
InvitationEmail__Smtp__Username=apikey
InvitationEmail__Smtp__Password=...
```

The connection string is normalized by the application to use a minimum pool size of zero, a
maximum pool size of 30, and ten-second idle/pruning settings. This lets the database become idle
once the Fly machine has been stopped.

Data Protection and desktop-protocol certificates, Stripe configuration, and at least one complete
external-login provider retain their existing environment-variable configuration. See
[`../fly/README.md`](../fly/README.md) for deployment and the full secret list.

`migrate` runs the committed EF migrations and exits:

```bash
dotnet run --project src/Styrhous.Licensing -- migrate
```

The web process never applies migrations implicitly. It validates the schema at startup.

## Background work and email

Rebus uses PostgreSQL (`RebusMessages`) for the work queue and error queue. The durable application
outbox remains the source of truth: a request publishes an immediate queue hint, while startup
recovery republishes pending or stale work. A stopped Fly application restarts on the next HTTP or
desktop request; this intentionally makes delayed retries opportunistic during the low-cost phase.

Invitation email is sent through SendGrid SMTP via MailKit. The durable delivery UUID is sent as the
non-secret `X-Styrhous-Delivery-Id` header. Recipient addresses, invitation secrets, email bodies,
and SMTP credentials are not logged.
