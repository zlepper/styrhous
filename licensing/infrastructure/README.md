# Licensing infrastructure

This Pulumi C# stack provisions the production AWS runtime for the licensing API,
portal, worker, database, queues, maintenance Lambda, secrets, and monitoring. It is
deliberately deployment-only: application schema changes run through the exported
one-shot migration task, followed by infrastructure-owned runtime-role provisioning, before API or worker rollout.

Configure the stack with immutable API and worker image URIs plus the required
secrets and an alert email address. AWS sends a confirmation request to that
address before the alarm subscription becomes active. `domainName`, `hostedZoneId`,
and `certificateArn` are optional, but must
be supplied together. Without them, the deployment workflow bootstraps the stack,
discovers CloudFront's generated HTTPS domain, and immediately reconciles the API
issuer and browser return URLs to that origin. The stack outputs the exact OAuth
callback URLs to register at each configured provider.

During certificate rotation, set the optional protected values
`desktopPreviousCertificates` and `dataProtectionPreviousCertificates` to JSON arrays whose
entries contain `certificate` and `certificatePassword`. Keep prior certificates configured until
their desktop credentials or browser data-protection payloads have expired.
The worker scales from zero to four tasks by default; set the optional
`maximumWorkerTasks` stack value to a number from 1 through 32 to change that cap.

The protected `Deploy licensing` workflow provisions repositories first and pushes
SHA-tagged immutable images. It then reconciles the full infrastructure while
excluding application compute and its dependents. This updates the one-shot
migration and provisioning tasks and all of their dependencies while any live API and worker remain on
their previous image and pinned secret version. After migration and runtime-role provisioning succeed, a
normal full-stack update rolls application compute forward. Portal upload,
CloudFront invalidation, and a same-origin health check normally happen after the
successful schema migration and application update. The workflow leaves all
infrastructure running. Fingerprinted assets from prior portal releases remain in
the private bucket so a browser holding an older HTML document never observes a
partially deleted release.

The stack stores the database owner and DML-only runtime credentials separately, along with each
runtime's configuration in its own
Secrets Manager secret. RDS Proxy can read only the two database credential secrets;
the API, worker, and maintenance runtimes can each read only their own configuration secret.
The administrative task role can read the separate migration and provisioning configuration secrets. Compute environments contain that secret's ARN and exact
version ID, never the connection string, certificates, OAuth credentials, or Stripe keys.
Changing a protected Pulumi value therefore creates a new secret version and a new
Lambda configuration or ECS task-definition revision that reads that version.
The database owner and runtime passwords are initialized by this workflow but cannot be rotated by an
ordinary application deployment: doing so would invalidate the credentials pinned
by the still-running application during migration. Use a separately coordinated
database-credential rotation before updating the deployment secret.

The protected staging preview preserves the API and worker image URIs already configured in its
Pulumi stack; it fails if that stack has not first been bootstrapped with deployable images. Public
API traffic is bounded at the HTTP API stage and by Lambda reserved concurrency so the direct API
Gateway hostname cannot bypass the same capacity ceiling.

The `release` GitHub environment must expose `STYRHOUS_HOSTED_LICENSE_ORIGIN`,
`LICENSING_PRODUCTION_PULUMI_STACK`, and `PULUMI_ACCESS_TOKEN`. A tagged desktop release compares
the protected origin to the selected production stack output and verifies the live desktop contract
before passing that exact origin to every package build. The validation environment may supply its
own `STYRHOUS_HOSTED_LICENSE_ORIGIN` for branch packages. When it does not, branch packages use
the reserved `https://licensing.validation.invalid` origin; tagged releases never use that fallback.

Run the complete infrastructure gate locally with:

```bash
bash check.sh
```

After a real AWS update, `aws-smoke-test.sh` verifies the deployed desktop OpenAPI, signing keys,
device-authorization contract, SPA/API routes,
an OAuth challenge and canonical callback, a signed Stripe webhook, the scheduled
maintenance Lambda path, Lambda-to-RDS startup, and the tagged alarm set. The
deployment workflow runs this automatically with the configured smoke OAuth provider.
`aws-operational-smoke-test.sh` then starts from zero worker tasks, seeds a durable maintenance
outbox probe, observes scale-up and a full no-scale-in interval while work remains in flight,
requires successful SES sandbox submission, and waits for the worker to return to zero. Finally it
adds and removes an exact marked DLQ message, requiring both a confirmed alert subscription and the
dead-letter alarm's ALARM/OK transition. Confirm the SES sender identity and SNS alert subscription
before the first protected deployment.

## Portal content security policy

SvelteKit embeds a hash-based CSP in the built static document. CloudFront adds
the portal's `frame-ancestors` policy from `portal/security-headers.json`; API
routes retain their separate restrictive policy. Publish and invalidate existing
portal documents with `publish-portal.sh --if-present` before updating the stack
only when the deployed document lacks CSP (the initial policy migration). Once
that migration is complete, publication happens after the backend update. The
workflow performs both checks. `production-csp.spec.ts` checks the built portal with the same
header and verifies that unauthorized inline scripts remain blocked.

## Runtime database access

`database-provisioning` is the infrastructure executable responsible for PostgreSQL runtime-role
creation, credential updates, and restricted grants. Pulumi defines its secret and ECS task; the
deployment runs `bash run-migration-task.sh provisioning` after migration and before runtime rollout.
Both administrative tasks run in private subnets without a public IP.

For a local provisioning smoke test after applying migrations, provide
`STYRHOUS_APPLICATION_CONFIGURATION` with `ConnectionStrings.Licensing` containing the owner
connection string and `DatabaseRuntimeRole.Username`/`Password` containing the runtime credential,
then run `dotnet run --project database-provisioning`. The container entrypoint is
`dotnet database-provisioning/Styrhous.Licensing.DatabaseProvisioning.dll`. Production uses a pinned
Secrets Manager reference instead of inline configuration. Neither path logs credentials.
