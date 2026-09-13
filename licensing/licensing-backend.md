# Styrhous Licensing System

## Purpose

Build a portable licensing platform for selling Styrhous. The system should keep initial SaaS
hosting costs low, remain suitable for a future expensive air-gapped offering, and avoid pretending
that source-available desktop software can be made tamper-proof.

Licensing is therefore an honor-based commercial signal. An expired or unlicensed installation
enters evaluation status and displays an appropriate warning; the backend does not remotely disable
Styrhous features.

The initial implementation includes the backend, customer portal, AWS deployment, portable
self-hosted architecture, versioned desktop protocol, and Rust desktop integration.

The original exploratory notes are in [GitHub issue #5](https://github.com/zlepper/styrhous/issues/5).

## Product decisions

- Sell recurring licenses per seat. An individual owns one personal seat; an organization purchases
  seats for its members. The per-seat price is identical for personal and organization customers.
- Offer monthly and annual SaaS subscriptions.
- Begin a 30-day trial automatically on a user's first successful signup. Starting a trial does not
  require a card, Stripe checkout, or a separate activation action.
- Allow one originated trial per verified person. A personal trial has one seat. It may be
  transferred once to the first organization created by its owner, retaining its original start and
  expiry, after which it supports up to five enabled or seat-bearing invited product seats.
- Use Stripe-hosted Checkout for purchases and Stripe Customer Portal for payment methods, billing
  details, tax IDs, invoices, and cancellation. Use Stripe Tax, invoicing, receipts, dunning, and
  billing notifications wherever Stripe can reasonably own the workflow.
- Add paid seats immediately with proration and immediate invoicing. A decrease is allowed only
  when the new capacity contains every product-enabled member and seat-bearing pending invitation;
  it gives no current-period credit and lowers the next renewal charge.
- Support GitHub, Google, and Microsoft social login in SaaS.
- Support a future self-hosted mode using customer-provided generic OIDC and a vendor-signed annual
  seat license.
- Default to three active desktop devices per seat. Self-hosted license policy may override this
  limit.

## Repository layout

All new licensing files must live beneath a single top-level `licensing/` directory to avoid
polluting the repository root. Organize that subtree internally into:

- the .NET solution and backend projects;
- the SvelteKit portal;
- Pulumi infrastructure;
- Docker Compose configuration;
- test fixtures and scripts; and
- licensing-specific documentation.

Only existing repository integration files may be changed outside this subtree where unavoidable,
notably `flake.nix`, `flake.lock`, and files under `.github/workflows/`. Do not add separate root-level
package, solution, Compose, environment, or tool configuration files.

Use centrally managed NuGet versions with lock files and commit the frontend package lock. Keep
generated artifacts, local environment files, and licensing-specific caches within the licensing
subtree or ignored build/cache directories.

## Technology and project architecture

### Backend

- ASP.NET Core 10 and .NET 10.
- EF Core 10 with PostgreSQL through Npgsql.
- ASP.NET Core Identity for application users and external logins.
- OpenIddict for the standards-based desktop device authorization flow and token lifecycle.
- Stripe.net for Stripe integration.
- Rebus with the Amazon SQS and RabbitMQ transports.
- A static client-only SvelteKit portal using TypeScript.
- Pulumi in C# with Pulumi Cloud for infrastructure state and encrypted configuration.

The approved external package set includes Npgsql EF, OpenIddict, Stripe.net, Rebus service/SQS/
RabbitMQ packages, official AWS Lambda and SDK packages, Pulumi AWS, and the SvelteKit static,
Vitest, and Playwright toolchain. NUnit and Microsoft.AspNetCore.Mvc.Testing are also approved for
backend tests. Prefer established, widely adopted ecosystem packages for cross-cutting concerns
such as OpenAPI/Swagger instead of building equivalent infrastructure from scratch. Do not add
Testcontainers or an external UI/component framework. Discuss niche or unusually consequential
dependencies separately before adding them.

Keep the deployed backend simple: use one ASP.NET Core Web SDK executable and one NUnit test
project. Separate domain, application, persistence, API, worker, maintenance, and migration concerns
with folders and namespaces inside the executable project. Select the API, worker, scheduled
maintenance, and explicit migration behaviors through command-line modes so Lambda, Fargate, and
deployment jobs can reuse one artifact. The Pulumi program remains a separate infrastructure project
because it has an independent build and deployment lifecycle.

Structure the .NET solution as a modular monolith with clear boundaries for:

- domain and application behavior;
- stable desktop contracts and internal application contracts;
- EF persistence and migrations;
- external identity, Stripe, email, messaging, and signing adapters;
- the ASP.NET API/Lambda host;
- the continuously hosted Rebus worker; and
- automated tests.

Keep Stripe, AWS, Rebus, ASP.NET, and deployment-specific types outside the domain model. Both SaaS
and self-hosted modes must use the same domain/application behavior and select infrastructure
adapters through configuration.

### Identifiers and persistence

Use UUIDv7 for every Styrhous-owned identifier, including users, billing accounts, organizations,
memberships, seats, invitations, trials, subscription projections, billing operations, audit
records, inbox/outbox entries, devices, activations, and sessions.

Generate UUIDv7 values through one shared application/EF value generator based on
`Guid.CreateVersion7()`. Preserve externally owned identifiers such as Stripe IDs, OAuth provider
subjects, and Rebus/SQS identifiers in their original string representation.

Use UTC `DateTimeOffset` timestamps. Treat EF Core as the repository and persistence abstraction:
application persistence code must use EF LINQ, bulk-update/delete APIs, transactions, and optimistic
concurrency rather than raw or interpolated SQL. Add bounded optimistic retries where capacity,
activation limits, or state transitions can race.
Database-backed test fixtures follow the same rule. Direct provider command execution is limited to
creating and deleting the ephemeral PostgreSQL databases themselves. EF command interceptors may
inspect provider-generated commands to coordinate deterministic races, but do not issue persistence
commands.

Use explicit EF-managed concurrency revisions on shared user, organization, and billing-account
roots when overlapping operations must have one stable order. Advance the relevant revision inside
the business transaction before reading the dependent state. This is an intentional coordination
mutation, not a self-assignment update: it serializes trial transfer, invitation capacity, device
limits, and subscription projections through EF without provider-specific lock queries. Operations
that touch more than one root claim them in user, organization, then billing-account order.

Provide a dedicated migration executable or deployment job. The API and worker must validate schema
compatibility on startup but must never apply migrations implicitly.

## Identity and browser authentication

Use ASP.NET Core Identity with GitHub, Google, and Microsoft external authentication. A new account
requires a verified email from the provider and is identified by the provider's stable subject ID.

Never merge accounts automatically because two providers report the same email. When an existing
email is encountered through an unlinked provider, direct the user to sign in with an already linked
provider and explicitly link the new identity after recent re-verification.

Keep portal authentication deliberately simple:

- The API handles external-provider redirects and callbacks.
- The API issues an HTTP-only, secure, first-party session cookie.
- The portal and API are served from one logical origin.
- The static SPA calls the API directly with the session cookie.
- Mutating requests use normal antiforgery protection.
- Do not introduce a BFF framework, token relay, or separate web gateway.

Internal mutating endpoints derive their actor UUID only from the authenticated name-identifier
claim and ignore any client-supplied actor field. They require an antiforgery request token issued
to the same authenticated browser session; both session and antiforgery cookies use secure
`__Host-` cookies, and token responses are never cacheable. API authentication challenges return
HTTP 401/403 rather than browser redirects.

The host issues external-login sessions through ASP.NET Core Identity. Persist Data Protection keys
in PostgreSQL, encrypt them with the configured certificate, and use one explicit application name
so cookies, antiforgery tokens, and protected outbox payloads remain usable across restarts and
processes. Require an RSA certificate and fail configuration when it is absent, invalid, or uses an
unsupported key algorithm. Certificate rotation installs the replacement as the active encryption
certificate while retaining every old certificate as decrypt-only until no persisted key-ring row
or protected payload depends on it. Do not log provider tokens, invitation secrets, session tokens,
or unnecessary personal data.

## Trial creation

On the first successful external signup, one database transaction creates:

1. the user;
2. their personal billing account;
3. their personal seat assignment; and
4. a 30-day trial beginning immediately.

The same transaction records the external identity and a verified-email claim. Keep historical
verified-email claims bound to the user when a stable provider subject reports a changed address;
this prevents either the old or new address from starting another trial through a different
provider. A claimed address owned by another user requires explicit account linking and must never
auto-merge accounts.

There is no trial-start page or button. A user invited to an existing organization still receives
their personal trial at signup, even if an organization entitlement currently takes precedence.

Enforce one originated trial per person with a database uniqueness constraint. Creating additional
organizations, changing providers, transferring ownership, or deleting an account must not reset
trial eligibility.

Account deletion must anonymize/deactivate the account rather than erase its trial history. Retain
only a deployment-keyed eligibility marker derived from the normalized verified email so
re-registering cannot create another trial while the user-facing email can be removed. Do not
expose account deletion until a real-database delete-and-register-again test proves this invariant.
Deletion must replace retained verified-email claim values with those markers rather than simply
removing the claim rows.

An active personal trial may be transferred once to the first organization its user creates. The
transfer changes the owning billing account but not trial start or end. The organization may then
hold at most five enabled product seats plus seat-bearing pending invitations during that trial.

## Accounts, organizations, seats, and invitations

Model personal and organization billing accounts uniformly where practical:

- A personal account has one owner and one seat.
- An organization has Owner, Admin, and Member roles.
- Every organization membership has one stable seat identity, but its product access may be enabled
  or disabled independently of its Owner, Admin, or Member role.
- Only a product-enabled organization seat consumes licensed capacity. Disabling it preserves the
  membership, seat identity, device limit, and device records.
- An invitation records whether a product seat will be enabled on acceptance. Only a seat-bearing
  pending invitation reserves licensed capacity.
- A person may belong to multiple organizations and have a distinct, independently enabled seat
  identity in each organization.

The unique `Owner` membership is the authority for organization ownership. Organization billing
accounts do not carry a separate user owner; personal billing accounts retain their dedicated
personal owner relationship. Keep the creator identity separately as immutable history.

Do not expose organization mutation endpoints until authenticated actor and request context are
available and organization creation, owner-membership creation, and any trial transfer write their
audit records in the same transaction. Organization creation also audits its initial seat assignment;
all records from the operation share one UUIDv7 correlation ID and the authenticated owner as actor.

Support organization creation, account switching, invitations, cancellation/resend, acceptance,
member removal, role changes, product-seat assignment, ownership transfer, and billing
administration. Organization creation enables the initial Owner product seat.

The internal `GET /api/organizations` endpoint lists only organizations in which the authenticated
user has a membership. It returns organization and billing-account identifiers, the user's role,
stable seat identity, product-seat assignment state, device capacity, and membership time in
deterministic membership order. Valid
users with no memberships receive an empty list; stale authenticated users receive HTTP 401.
Responses are not cacheable.

The internal `GET /api/organizations/{organizationId}/members` endpoint requires a UUIDv7
organization identifier and lets any current organization member view the roster. Unknown
organizations and callers without a membership both return `organization_not_found`, so the route
does not disclose organization existence; stale authenticated users receive HTTP 401. The
non-cacheable success response returns `organization_members_listed`, the organization identifier,
and every member's membership, user, and seat UUIDv7 identifiers, current verified email, role,
product-seat assignment state, device limit, and join time. Sort members by join time and then
membership identifier for a
stable result. Pending invitations are deliberately not part of this member-visible response and
remain an administrator concern.

Remove a membership, or leave an organization, through the internal
`DELETE /api/organizations/{organizationId}/members/{membershipId}` endpoint. The route requires
UUIDv7 organization and membership identifiers, authentication, and antiforgery protection. A
non-member receives the same `organization_not_found` response as an unknown organization. Missing,
cross-organization, already-removed, malformed, and non-UUIDv7 membership identifiers return
`organization_member_not_found` after the actor's membership is authorized.

Apply this authorization policy:

- an Admin or Member may remove their own membership;
- an Admin may remove another Member;
- the Owner may remove an Admin or Member; and
- no one may remove the Owner, including the Owner themselves, until ownership is transferred.

Unauthorized removal returns `insufficient_permission`; attempting to remove the Owner returns
`ownership_transfer_required`. On success, hard-delete the target membership, its organization seat,
and all device-activation rows attached to that seat in one transaction. Do not delete the user,
their personal seat, memberships in other organizations, or prior audit records. Add one immutable
`DeviceActivationRemoved` audit record for each deleted activation, plus `SeatUnassigned` when the
product seat was enabled and `OrganizationMemberRemoved` in every case, all sharing the
authenticated actor, operation time, and one
UUIDv7 correlation identifier. The non-cacheable response contains `organization_member_removed`,
the organization, membership, user, seat, and correlation UUIDv7 identifiers, and removal time; it
does not expose removed device details.

Serialize removal against device and organization changes in one EF-managed transaction. Revalidate
the organization-scoped target and use conditional updates or deletes as optimistic concurrency
checks, retrying the policy decision after a conflict. This makes concurrent removal idempotent,
prevents activation from surviving a removed seat, and makes released member or invitation capacity
available immediately.

Change a non-owner's role through
`PATCH /api/organizations/{organizationId}/members/{membershipId}/role` with an `admin` or `member`
role. Only the current Owner may use this endpoint. Changing the Owner through it returns
`ownership_transfer_required`, and requesting the member's existing role returns
`organization_member_role_unchanged`. Transfer ownership through
`POST /api/organizations/{organizationId}/members/{membershipId}/transfer-ownership`; only the
current Owner may transfer to an existing Admin or Member, and the former Owner becomes an Admin.
The target may not be the current Owner.

Both routes require UUIDv7 identifiers, authentication, and antiforgery protection and preserve the
same organization-privacy and member-not-found behavior as removal. They keep every seat and device
assignment unchanged. Run each mutation and its audit records in one EF-managed transaction. Use
conditional EF updates as optimistic concurrency checks and reload the policy decision when an
affected-row count is zero, so concurrent transfers leave exactly one Owner. A role change writes
`OrganizationMemberRoleChanged`; a transfer writes that action for the former Owner plus
`OrganizationOwnerAssigned` for the new Owner, sharing one UUIDv7 correlation identifier. Successes
return `organization_member_role_changed` or `organization_ownership_transferred` with the affected
membership/user identifiers, previous and current role or owner, correlation identifier, and UTC
operation time.

Assign or remove product access through
`PATCH /api/organizations/{organizationId}/members/{membershipId}/seat` with an `assigned` boolean.
An Owner or Admin may change any member's product access, including the Owner, themselves, or a peer
Admin. A Member receives `insufficient_permission`; an unchanged request returns
`organization_member_seat_unchanged`. Enabling requires an active transferred trial or eligible
commercial subscription and an available slot after product-enabled seats and seat-bearing active
invitations are counted. Otherwise return `no_active_seat_capacity` or `seat_capacity_reached`.

The mutation retains the membership, seat UUID, device limit, activations, and desktop-session rows.
It changes only the product-access flag and writes `SeatAssigned` or `SeatUnassigned` in the same
serialized transaction. Disabling makes fresh entitlement checks ineligible; already-issued short
access tokens and offline leases expire normally. Enabling restores the same seat identity, though
a device whose refresh chain was revoked by an ineligible refresh attempt may need to authenticate
again. The non-cacheable success returns the organization, membership, user, seat, and correlation
UUIDv7 identifiers, the resulting assignment state and device limit, and the UTC change time.

The internal `GET /api/organizations/{organizationId}/invitations` endpoint requires a UUIDv7
organization identifier and lets Owners and Admins list only active pending invitations. A Member
receives `insufficient_permission`; unknown organizations and callers without a membership both
receive `organization_not_found`; stale authenticated users receive HTTP 401. The non-cacheable
success response returns `organization_invitations_listed`, the organization identifier, and each
invitation's UUIDv7 identifier, creator UUIDv7, email, role, creation and most-recent-send times, and
whether it will assign a product seat, plus its expiry. It never returns the normalized email,
secret hash, or raw secret. Sort invitations by
creation time and then invitation identifier. Accepted, cancelled, and expired invitations are
excluded, with an invitation considered expired at its exact expiry time.

Invitations expire after seven days. Accepting an invitation requires that the signed-in user's
verified provider email match the invited email. Store only a hash of the invitation secret. Role,
membership, invitation, and billing changes must create audit records.

The internal invitation-creation contract is
`POST /api/organizations/{organizationId}/invitations`. It accepts an email and an `admin` or
`member` role plus an optional `assignProductSeat` boolean that defaults to `true`. The route
requires a UUIDv7 organization identifier, an authenticated Owner or Admin, and a valid antiforgery
token. Non-members receive the same `organization_not_found` response as an
unknown organization so the endpoint does not disclose organization existence. Member callers
receive `insufficient_permission`.

Creation normalizes the invitee email and rejects an existing member or another unexpired pending
invitation. A seat-bearing invitation reserves capacity until the seven-day expiry; a seatless
invitation may be created without an active licensing plan and reserves zero capacity. During a
transferred trial, product-enabled seats plus seat-bearing pending invitations may not exceed five.
An eligible `active` or `past_due` commercial
subscription instead uses its purchased seat quantity, which takes precedence over a larger active
trial capacity. Serialize invitation creation in an EF-managed transaction that reads authorization
and capacity consistently and conditionally inserts the reservation. Retry the complete decision
after serialization or concurrency conflicts so concurrent Owner/Admin requests and
subscription-quantity projections cannot overbook the last seat. Return
`no_active_seat_capacity` when neither an eligible paid subscription nor an active transferred
trial can fund a new reservation.

Persist the positive seat-capacity ceiling that funded each seat-bearing invitation as
`reserved_seat_capacity`; persist zero for a seatless invitation. Enforce the correspondence between
the invitation's product-seat choice and its reservation at the database level. A positive
reservation remains independent of later subscription status,
period, or quantity reductions while its invitation window remains active. A later capacity
increase may promote active reservations but never reduces them. Whenever a larger ceiling admits
a new or reactivated invitation, promote every earlier active reservation to that same ceiling so
acceptance and resend cannot depend on which valid invitation is processed first.

A successful response is non-cacheable and contains only `invitation_created`, the invitation and
correlation UUIDv7 identifiers, and the expiry time. It must never contain the raw invitation
secret. Conflict reason codes are `already_member`, `invitation_already_pending`, and
`seat_capacity_reached`. Persist the invitation hash and `OrganizationInvitationCreated` audit
record atomically; later outbox/email work must carry the raw secret directly into its protected
delivery payload without adding it to the invitation row, API response, or logs.

Cancel an invitation with
`DELETE /api/organizations/{organizationId}/invitations/{invitationId}`. Apply the same UUIDv7,
authentication, antiforgery, organization privacy, and Owner/Admin authorization rules as creation.
Only an unaccepted, uncancelled invitation whose seven-day window is still active may transition.
Unknown, cross-organization, expired, accepted, already-cancelled, and non-UUIDv7 invitation
identifiers all return `invitation_not_found`; cancellation is therefore safe to retry without
revealing another invitation's state. Cancellation remains available after the organization's
trial or paid entitlement expires so administrators can release stale reservations.

Read the authenticated user, organization, and invitation in one EF-managed transaction. Apply the
cancellation only when a conditional EF update still matches the invitation state used by the
policy decision, retrying conflicts. Persist the cancellation timestamp and
`OrganizationInvitationCancelled` audit record in the same transaction.
The non-cacheable success response returns `invitation_cancelled`, the invitation and correlation
UUIDv7 identifiers, and the cancellation time. A cancellation immediately removes both the
normalized-email duplicate block and the invitation's reserved-seat count.

If a concurrent resend with a later observed time commits before an older cancellation completes
its conditional transition, the cancellation reloads and returns
`invitation_cancellation_superseded` without changing the refreshed invitation or writing a
cancellation audit record. The caller may reload and decide whether to cancel the newly resent
invitation.

Resend an invitation with
`POST /api/organizations/{organizationId}/invitations/{invitationId}/resend`. Apply the same UUIDv7,
authentication, antiforgery, organization privacy, and Owner/Admin authorization rules as creation
and cancellation. Resend updates the existing invitation rather than creating another reservation:
it keeps the invitation identifier, email, role, product-seat choice, creator, and creation time,
rotates the secret hash, sets `last_sent_at` to the current time, and starts a new seven-day expiry
window. Changing a pending invitation's product-seat choice requires cancellation and a new
invitation. Cancelled and
accepted invitations are terminal. Unknown, cross-organization, cancelled, accepted, and non-UUIDv7
invitation identifiers return `invitation_not_found` without revealing their state.

An active seat-bearing invitation already owns its seat reservation, so exclude it from both the
duplicate-email and capacity counts during resend and reuse its persisted
`reserved_seat_capacity`, even if its funding entitlement has since expired or changed. A seatless
invitation remains unreserved. An expired seat-bearing invitation may be reactivated only
when its invitee has not become a member, no other active invitation reserves the same normalized
email, the organization has an active entitlement, and one seat is available; a successful expired
resend records the newly resolved capacity. Otherwise return the same
`already_member`, `invitation_already_pending`, `no_active_seat_capacity`, or
`seat_capacity_reached` conflict used by creation. Recheck these conditions in one EF-managed
transaction and condition the invitation transition on its previously observed state so concurrent
reactivations cannot overbook capacity. Reload and retry the policy decision after a conflict.

Persist the rotated hash, refreshed send/expiry timestamps, and
`OrganizationInvitationResent` audit record atomically. The non-cacheable success response returns
only `invitation_resent`, the existing invitation identifier, a new correlation UUIDv7, and the new
expiry. It never returns the raw replacement secret. Later outbox/email work must consume the raw
secret directly from the application result in the same logical operation and must not persist it
on the invitation row or write it to logs. If a concurrent resend with a later observed time commits
first, the older request returns `invitation_resend_superseded` without rotating the secret or
writing another audit record; this prevents a stale request from failing with a server error or
invalidating the newer delivery.

Accept an invitation with authenticated `POST /api/invitations/accept`. Put the raw invitation
secret only in the JSON request body as `secret`; require the normal antiforgery token, reject blank
or greater-than-256-character values, mark every response `Cache-Control: no-store`, and never put
the raw value in a path, database row, response, audit record, or log. Hash the submitted value
before lookup. Unknown, wrong, expired, cancelled, already-accepted, and rotated secrets all return
the same `invitation_not_found` response so terminal state and secret validity are not disclosed.

The authenticated user may accept when any verified email claim retained for that user matches the
invitation's normalized email; it need not be the provider's current primary email. A mismatch
returns `invitation_email_mismatch`. An existing membership or seat assignment for the same
organization returns `already_member`. Acceptance remains permitted while the invitation is active.
A seat-bearing invitation remains funded even if the organization's trial or paid entitlement has
since expired because creation or resend already reserved its capacity; a seatless invitation never
requires that capacity.

Serialize acceptance in an EF-managed transaction by resolving the secret hash without exposing it,
loading the authenticated user, organization, invitation, assigned seats, and active reservations,
and conditionally updating the invitation from its previously observed state. Revalidation must
include the hash and active seven-day window so a concurrent resend invalidates the old link and
concurrent acceptance/cancellation produces exactly one terminal transition. Sample the clock when
evaluating the conditional transition for the active-window decision and acceptance timestamp. In
the same transaction, for a seat-bearing invitation also recheck enabled product seats plus every
other active seat-bearing invitation against the target invitation's persisted
`reserved_seat_capacity`. This preserves the
specific capacity promise under which it was created or last reactivated, even after the funding
entitlement expires or its projected quantity changes. Resolve mixed active reservations at their
largest promoted ceiling and promote the remaining active reservations when one is accepted. If the
available slot has already been reallocated, return
`invitation_not_found`; this prevents a delayed or clock-skewed instance from overbooking after the
target invitation expires. In one PostgreSQL transaction, mark the invitation accepted by the user,
create the invited Admin or Member membership and stable seat identity with the default three-device
limit, enable that seat only when requested by the invitation, and write
`OrganizationInvitationAccepted` and `OrganizationMemberAssigned` audit records plus `SeatAssigned`
only for an enabled product seat.
All newly owned identifiers and the shared audit correlation identifier are UUIDv7.

The successful non-cacheable response contains only `invitation_accepted`, the invitation,
organization, membership, seat, and correlation UUIDv7 identifiers, the accepted role, resulting
product-seat assignment state, and UTC acceptance time. It does not return the invitation secret or
hash.

## Device authorization and activation limits

Each product-enabled personal or organization seat permits three active desktop devices by default.
Device limits
are scoped per seat, not globally per person. A person licensed by two organizations may therefore
have up to three activations attached to each of their two organization seats.

During browser approval of a device authorization:

- If the user has one eligible seat, select it automatically.
- If the user has multiple eligible seats, require them to choose the personal or organization seat
  that will own the activation.
- Show the selected seat and its active-device capacity.

Each desktop installation supplies a UUIDv7 installation ID, a user-editable display name, and
non-sensitive platform, architecture, and Styrhous-version metadata. Refresh sessions and
entitlement checks are bound to the resulting device activation.

Persist the default limit on each seat so policy can be overridden later. An installation may have
one active activation per seat; retrying that same pair is idempotent and does not transfer it from
another seat. Treat activation time as the initial `last_seen_at`. A replacement is stale only when
`last_seen_at` is strictly earlier than the seven-day cutoff. Write the stale revocation and its
replacement activation with generic, append-only audit records in one transaction under one UUIDv7
correlation ID. Resolve entitlement and change the activation set in one EF-managed transaction,
using conditional EF mutations or concurrency tokens for the selected seat, billing account, and
activation rows. Reload and retry the complete policy decision after conflicts with trial transfer
or subscription projection updates. Read fresh database state within every activation operation. If
a future policy change leaves a seat above its reduced limit, reject further activation until seat
management explicitly reconciles capacity.
Browser approval creates the activation and its OpenIddict authorization/session link in the same
serializable transaction that accepts the user code. Explicit, stale-replacement, membership-removal,
and entitlement-loss revocations invalidate the linked authorization and token chain atomically.
OpenIddict owns the device-code, JWT access-token, rotating refresh-token, and revocation protocol
machinery. Protect it with a dedicated signing/encryption certificate ring rather than broadening
the existing Data Protection certificate contract; the latter remains encryption-only and protects
the shared PostgreSQL key ring. Require one canonical HTTPS desktop issuer/public origin and derive
protocol issuers and browser verification links from it rather than the transport host. The current
desktop certificate must expire after every retained previous certificate so OpenIddict selects the
current key for new protocol state, and it must be within its validity window at startup. Begin the
serializable protocol transaction only after a form body has been read under a 16 KiB request limit,
then keep OpenIddict token mutation and Styrhous
device mutation inside that transaction. Buffer protocol responses until the database commit has
succeeded. Serialize refresh and revocation decisions through the owning user row and retry complete
EF activation/revocation and refresh-reuse recovery operations after bounded concurrency failures.
Default request logging omits query strings so the browser approval `user_code` is not written to
application logs.

An entitlement check is scoped to the authenticated user and one active activation. Resolve the
selected seat's entitlement from fresh database state. Eligible checks advance `last_seen_at` at
most once per hour, including at the exact one-hour boundary. Ineligible checks return the current
evaluation reason without extending device activity. Unknown, revoked, and other users' activation
IDs share one `device_not_active` result so the check cannot disclose device ownership. Serialize
checks with activation, trial-transfer, and subscription-projection changes through the same
EF-managed transaction and optimistic-concurrency protocol.

Manual revocation targets one active activation owned by the authenticated user. It uses the same
EF optimistic-concurrency protocol, writes the revocation and a generic audit record atomically under
one UUIDv7 correlation ID, and immediately frees that seat's capacity. Repeated, unknown, revoked,
and other users' IDs
share the `device_not_active` result and never create another audit record. Revoking the associated
OpenIddict refresh session is part of the same transaction. If a request waits behind a newer
successful entitlement check, clamp its revocation and audit timestamp to the device's latest
`last_seen_at` so stored time never moves backward.

The internal `DELETE /api/device-activations/{activationId}` endpoint exposes manual revocation to
the portal. It requires the authenticated browser session and antiforgery token and returns a
non-cacheable correlated result. Malformed, non-UUIDv7, unknown, revoked, and other users'
activation identifiers all return the same HTTP 404 body containing only `device_not_active`.

Active-device listing is scoped to an authenticated user and one owned seat. Return the persisted
seat capacity and active devices ordered by most recent `last_seen_at`, excluding all revoked
activations. Read both through one repeatable-read snapshot so the capacity and device set are
internally consistent without blocking activation writes. Listing remains available after
entitlement expiry or while an organization product seat is disabled, so a user can inspect and
clean up retained devices. Disabling a product seat blocks those activations from entitlement use
until the same seat is enabled again. Unknown and other users' seat IDs share one `seat_not_found`
result without capacity or device details.

The internal `GET /api/seats/{seatId}/devices` endpoint exposes that listing to the portal with
non-cacheable responses. Malformed, non-UUIDv7, unknown, and other users' seat identifiers all return
the same HTTP 404 body containing only `seat_not_found`.

When an activation would exceed a seat's device limit:

1. Read the seat's activation set in an EF-managed transaction.
2. Find an active device whose last successful entitlement check was more than seven days ago.
3. Revoke the least-recently-seen eligible device and its refresh session, recording the reason in
   the audit log.
4. Conditionally commit the revocation and new activation, reloading and retrying after conflicts.
5. If no device is stale, return the stable `device_limit_reached` result. The approval page must
   list the active devices and allow the user to explicitly revoke one before retrying the pending
   approval without repeating social login.

Concurrent approvals must never exceed the configured capacity. Update `last_seen_at` after a
successful desktop entitlement check, but write at most once per device per hour.

Removing an organization membership hard-deletes the organization seat and its activation rows;
future refresh sessions attached to them must be invalidated in the same transaction. Disabling the
product seat instead retains those rows. A later ineligible refresh or entitlement-loss check, or
manually revoking a device, invalidates its refresh session. An already issued offline lease remains
valid until its short expiry. If the user still owns another eligible seat,
the next activation flow allows them to select it; activations are not silently transferred between
organizations.

## Entitlement model

Derive user-facing entitlement states rather than exposing raw Stripe or license data:

- `evaluation`: no currently valid trial or commercial entitlement;
- `trial`: active internal trial;
- `commercial`: active paid SaaS or valid self-hosted seat; and
- `grace`: a commercial entitlement requiring billing or renewal attention.

Treat validity windows as half-open: the start instant is eligible and the end instant is not.
The initial trial resolver uses the stable reason codes `active_trial`, `trial_not_started`,
`trial_expired`, and `no_valid_entitlement`. A disabled organization seat always resolves to
`evaluation` with `product_seat_not_assigned`, no validity window, and ineligible access regardless
of the underlying plan. The resolver assesses every seat identity assigned to the user so a
transferred trial follows its billing account and cannot continue licensing the abandoned personal
seat. Only `trial`, `commercial`, and `grace` assessments are eligible for device seat selection.

Return a stable reason/warning code alongside the state, the source billing account and seat, the
validity window, device information, and a purchase or management URL.

The internal `GET /api/entitlements` endpoint derives the user only from the authenticated session
and returns every assigned seat assessment in deterministic persistence order. It exposes lowercase
state names, the reason code, eligibility, and validity window for direct portal use. Responses for
authenticated requests are not cacheable, and a stale authenticated user receives HTTP 401 without
entitlement details.

For Stripe subscriptions:

- `active` is commercial.
- `past_due` remains commercial in grace/attention while Stripe performs configured retries.
- cancellation at period end remains commercial until the paid period ends.
- `unpaid`, `paused`, `incomplete_expired`, or fully expired/cancelled becomes evaluation.

Persist one current commercial-subscription projection per billing account under its own UUIDv7.
Keep the externally owned customer, subscription, and Price identifiers as bounded strings alongside
status, purchased seat quantity, cancellation-at-period-end, the half-open current paid period, the
provider-read revision, and the last projection time. Use Stripe's response time as the primary
snapshot order, and reserve a provider-read revision after every successful authoritative
subscription response to break equal-time ties durably across API and worker replicas and process
restarts. External customer and subscription identifiers are
unique. An eligible commercial projection takes precedence over an internal trial; an incomplete
or otherwise ineligible projection does not suppress a still-active internal trial. Use the stable
reason codes
`active_subscription`, `subscription_past_due`, `subscription_cancels_at_period_end`,
`subscription_not_started`, `subscription_expired`, and `subscription_inactive`.

An eligible organization subscription funds no more than its purchased seat quantity. Consider
only product-enabled seats. Fund the organization's Owner first when that seat is enabled, then fund
the remaining enabled seats by `created_at` and UUID. This makes a partial projection deterministic across API,
activation, and entitlement-check requests. Assigned seats beyond the projected quantity return
evaluation with the stable `subscription_seat_capacity_exceeded` reason and the commercial
projection's validity window. An eligible but over-capacity commercial seat does not fall back to
the organization's active internal trial. Ineligible commercial projections still allow the normal
trial fallback. Resolve the ordered funding set from one consistent database snapshot; device
activation and entitlement checks use the same billing-account concurrency token or conditional
state predicate that serializes them with subscription projections.

Normal portal-initiated seat decreases remain blocked when the requested quantity would not contain
every product-enabled member and active seat-bearing invitation. The deterministic partial-funding
rule still handles
an authoritative provider projection below the local assignment count without licensing an
arbitrary subset or returning inconsistent answers.

Order authoritative provider snapshots first by Stripe's HTTP response time. For equal response
times, a successful mutation response outranks an ordinary observation; snapshots of the same kind
are then ordered by a durable database-backed read revision reserved after Stripe returns. This
prevents Stripe's one-second HTTP-date precision and a delayed GET from allowing an older
observation to overwrite the response to a successful mutation. Ignore an older ordering tuple
without changing projection identity. A differing observation tied with a stored mutation response
is causally ambiguous rather than stale: leave the mutation response projected, release the webhook
lease, and retrieve Stripe again instead of marking that event processed. Retry revision-row
optimistic conflicts until request cancellation rather than turning ordinary contention into a
provider failure. Unversioned internal fixtures retain the legacy rule that only a strictly newer
projection time applies. Serialize each billing account's projection updates with an EF concurrency
token, then reload and retry conflicts. Webhook and reconciliation adapters must first retrieve
Stripe's authoritative current subscription rather than trusting potentially out-of-order event
payload state.

Attach the local billing-account UUID to every managed Stripe subscription as the
`styrhous_billing_account_id` metadata value. Webhook processing treats subscriptions without that
metadata key as unmanaged and ignores them. A present value must be a valid UUIDv7 and reference an
existing billing account; malformed or dangling managed metadata fails processing for retry and
eventual operator review rather than silently dropping entitlement changes.

A self-hosted license receives a 30-day renewal grace after its signed validity period. After grace,
the server returns evaluation status rather than blocking the product.

## Stripe billing

Trials remain internal and do not create Stripe customers or subscriptions. When a trial user
chooses to purchase, create the paid Stripe subscription immediately. End the internal trial in
the same EF transaction that first projects an authoritative, currently eligible `active` or
`past_due` paid period; merely creating or returning from Checkout does not prove payment and must
not terminate the trial. Preserve the original trial end while recording its earlier termination
time so audit and support views retain the complete trial history.

Configure the monthly and annual Stripe Price IDs on the server. Personal and organization accounts
must use the same Price ID for a given cadence. Never accept a price amount or arbitrary Price ID
from the browser. Webhook and reconciliation projections must also fail closed unless the
authoritative subscription item uses one of those exact configured Price IDs.

Use Stripe-hosted Checkout for initial purchases and Stripe Customer Portal for:

- payment methods;
- invoices and receipts;
- billing address and tax IDs;
- cancellation; and
- other billing operations Stripe can safely own.

Keep membership, invitation, and seat quantity management in Styrhous because Stripe cannot enforce
the product-enabled-plus-seat-bearing-reservation capacity rule.

`POST /api/billing-accounts/{billingAccountId}/checkout-sessions` accepts only a server-defined
monthly or annual cadence, a positive integer seat quantity, and an optional prior billing-operation
UUIDv7 for retry. Personal Checkout is fixed at one seat. Organization Checkout is Owner-only and
must cover every product-enabled member plus active seat-bearing invitation; there is no artificial
product maximum
beyond the positive integer representation. Unknown accounts and organizations the caller cannot
see share the normal account-not-found response.

Before contacting Stripe, persist a pending initial-Checkout operation and its immutable audit
record in one EF-managed transaction. The operation contains only local UUIDv7 identifiers,
cadence, quantity, and a one-hour provider-session window. Enforce at the database level that each
billing account has at most one pending or provider-created Checkout operation. Commit that
transaction, then call Stripe outside the database transaction with an idempotency key derived
solely from the operation UUID and configure the Stripe session to expire at the same instant.

A provider failure returns the same operation UUID to the portal, whose retry supplies it unchanged.
An identical fresh request during the live window also recovers that operation, covering a lost HTTP
response, process interruption, or concurrent duplicate without creating a second subscription.
A changed fresh request returns `checkout_operation_in_progress` with the safe cadence, quantity,
operation UUID, and expiry needed to continue the existing attempt. Once the provider-matched window
has ended, retrieve the recorded Stripe Session before releasing the account: a completed or still
open Session blocks replacement, and only Stripe-confirmed expiry closes it. For a completed Session,
retrieve its current subscription, apply the authoritative projection, terminate any superseded
trial, and close the Checkout operation in one EF-managed transaction. This recovers access even if
the webhook was missed without exposing a projected-subscription/live-Checkout split state. If a
prior request never reached Stripe, a retry using the
same idempotency key can receive a definite `expires_at` rejection; record that creation failure
before starting one replacement. Never infer abandonment from the local clock alone. All operation
transitions take the billing-account lock. Revalidate current ownership, subscription absence,
personal quantity, and product-enabled plus seat-bearing-reserved organization capacity on every
retry. If new assignments or reservations make an already issued
session too small, return `checkout_operation_capacity_changed` and its expiry; do not call Stripe
again or create a potentially duplicate replacement until that session is guaranteed expired.
Reject a retry if its authenticated actor, account, cadence, or quantity differs from the durable
operation. Once Stripe returns, record the bounded session identifier and local observation time;
recording the same provider Session more than once remains idempotent even if a duplicate response
arrives after provider-confirmed expiry.

The Checkout adapter selects only the configured monthly or annual Price ID, sends exactly the
validated quantity, enables Stripe Tax, copies the local billing-account UUID to subscription
metadata, and places the local operation UUID on both session and subscription metadata. Success
and cancellation destinations are trusted configured HTTPS URLs, never browser-supplied values.
The API returns only Stripe's HTTPS redirect URL, not Price, customer, or subscription identifiers.

`POST /api/billing-accounts/{billingAccountId}/customer-portal-sessions` takes no request body.
It requires the normal authenticated session and antiforgery token, and authorizes the current
personal owner or organization Owner from fresh EF-projected state. Unknown accounts and
organizations the caller cannot see return `billing_account_not_found`; a visible non-Owner
returns `insufficient_permission`; and an authorized account without a local subscription returns
`subscription_not_found`. The Stripe customer identifier comes only from the authorized local
commercial-subscription projection. The browser cannot supply a customer, Portal configuration,
or return destination.

Create the Stripe Customer Portal session against one server-configured, bounded configuration ID
and one trusted HTTPS return URL. Before every session, retrieve that exact configuration from
Stripe and fail closed unless it is active and its entire subscription-update feature is disabled:
Styrhous owns seat changes and their assigned-plus-reserved capacity invariant. Then fail closed
unless Stripe returns the expected customer, configuration, and return URL plus a bounded session
ID and HTTPS redirect. Network failures, provider timeouts, rate limiting, conflicts, and Stripe 5xx
responses return `customer_portal_provider_unavailable`. Permanent Stripe rejection is an operator
fault: record only its status, error type/code, and Stripe request ID, then return the normal server
error without logging Stripe's response or customer identifiers. Success returns only
`customer_portal_session_created` and the HTTPS redirect URL. Session creation does not create a
durable billing operation because it has no local billing side effect and is safe to request again.

`PATCH /api/billing-accounts/{billingAccountId}/seat-quantity` accepts a positive integer target
quantity and an optional prior billing-operation UUIDv7 for retry. Personal subscriptions remain
fixed at one seat. Organization changes are Owner-only, and the target must contain every enabled
product seat plus active seat-bearing invitation at the time the operation starts and on every
retry. There is no
artificial product maximum beyond the positive integer representation. Apply the normal
billing-account privacy and authorization responses, require an existing commercial-subscription
projection, and reject an unchanged target without contacting Stripe.

Persist a pending seat-quantity-change operation and its immutable audit record in one EF-managed
transaction before contacting Stripe. The operation binds the actor, billing account, previously
projected quantity, and requested quantity. Permit at most one live billing operation per account.
An identical request by the current authorized Owner recovers the live operation, including after a
lost HTTP response, ownership transfer, or subscription eligibility change; the initiating actor
remains immutable audit provenance rather than becoming a permanent recovery requirement. A changed
request returns `seat_quantity_operation_in_progress` with the safe operation UUID and quantities
needed to resume it. Once the operation is terminal, a fresh request re-evaluates current
subscription eligibility normally. A pending decrease temporarily becomes the account's effective
capacity for membership and invitation reservations, while a pending increase grants no capacity
until its authoritative provider projection commits. Persist the observation that authorizes a
mutation first. Immediately before sending that mutation, reacquire the billing-account lock and
verify that the exact observed provider snapshot is still current. Hold the lock while Stripe
rechecks and applies the update, then project the response and resolve the operation in that same
EF-managed transaction. This deliberately trades a short transaction across one provider call for
preventing a stale authorized request from overwriting a newer known subscription state.

Call Stripe using an idempotency key derived from the billing-operation UUID. First retrieve and
validate the exact managed subscription, configured
Price, single subscription item, metadata billing-account UUID, and current quantity. If Stripe is
already at the requested quantity, treat the retry as applied. If its current quantity differs from
both the operation's previous and requested quantities, project that authoritative state, close the
superseded operation, and return `subscription_quantity_changed` so the portal reloads before
trying again. Otherwise update the one existing subscription item. An increase uses
`proration_behavior=always_invoice` and explicitly allows the subscription to become `past_due` if
immediate collection fails, matching the existing commercial grace policy. A decrease uses
`proration_behavior=none`, gives no current-period credit, and changes the next renewal quantity.
Never change the Price, billing-cycle anchor, metadata, or any browser-supplied provider value.

Persist every authoritative observation under the billing-account lock before sending an update.
An ineligible subscription is superseding even when its quantity still equals the operation's
previous or requested quantity. If Stripe still reports the previous eligible quantity, an exact
retry may replay the identical update with the same operation-derived idempotency key until 23 hours
after the first provider observation that authorized the mutation. Persist Stripe's HTTP response
time as the replay-window start and compare it with each retry's Stripe response time, keeping one
clock basis across API replicas. Recheck the deadline against the final Stripe GET inside the
mutation lock; if that response reaches the cutoff, project it and do not POST. The operation's
creation time does not shorten that window. Never rotate the key for a live operation. Once the
window closes and Stripe still reports the previous quantity, apply that final observation but keep
the original operation pending for reconciliation; do not send the old update again and do not
authorize a fresh key. Return
`seat_quantity_reconciliation_required` with the original operation identity so later exact retries
can safely observe an eventually applied target or superseding quantity without risking duplicate
proration or invoice side effects. The portal retains that identity and directs the Owner to check
again later or contact support.

Validate the returned authoritative subscription as strictly as webhook and reconciliation input,
require the requested quantity, apply its projection synchronously, and only then close the durable
operation as completed. A network failure, provider timeout, rate limit, or conflict keeps the
operation pending and returns `seat_quantity_provider_unavailable` with its UUID so the same
request can be retried safely. Because Stripe caches `500` responses under an idempotency key, the
next retry retrieves authoritative state before replaying that same cached request: it accepts an
already-applied target or returns a superseding quantity. Never issue the operation under a fresh
idempotency key; a webhook projection can also provide the eventual authoritative state. A
permanent provider rejection closes the operation with a durable provider-rejected outcome, records
only bounded provider diagnostics, and returns the normal server error on both the original request
and exact retries. Cancellation of the local HTTP request leaves the operation
pending. Concurrent closure always applies the newest authoritative projection and returns the
operation's durable terminal outcome. An exact retry of a terminal operation returns that outcome
and current projected quantity without contacting Stripe. Success returns `seat_quantity_changed`,
the operation UUID, and the authoritative purchased quantity; the portal then reloads the account
portfolio.

Checkout and Customer Portal session creation are synchronous because the browser needs an
immediate redirect URL. Other billing operations may expose a pending operation in the UI and
complete asynchronously.

Stripe is the subscription system of record. The PostgreSQL subscription record is a local
projection:

- Verify webhook signatures against the untouched request body.
- Persist and deduplicate by Stripe event ID before returning success.
- Process only the required checkout, subscription, invoice, payment-failure, and cancellation
  event types.
- Claim pending inbox rows with a five-minute UUIDv7 processing lease guarded by EF's PostgreSQL
  `xmin` optimistic-concurrency token. Release the lease on failure, permit reclaim after expiry,
  and mark the row processed only after its projection has committed.
- Retrieve the Stripe Event by its stored identifier only to validate its event kind/object shape
  and locate the subscription. Then retrieve the current Subscription and project that
  authoritative snapshot. A supported event with no subscription is terminally ignored.
- Handle duplicate and out-of-order events idempotently.
- Reconcile active subscriptions periodically to repair missed or stale projections.
- Use stable Stripe idempotency keys for every retried mutation.

## Messaging, outbox, and scale-to-zero worker

Store business changes and outbox entries in one PostgreSQL transaction. The outbox record is the
durable source of truth for asynchronous work.

Protect invitation-delivery payloads with a dedicated, stable ASP.NET Data Protection purpose and
an explicitly versioned inner payload before they are written to the outbox; the raw invitation
secret and recipient email must not be stored as plaintext. Retain deserialization support for
every version that can remain in the durable outbox during rolling upgrades. The API and worker
share the deployment's PostgreSQL-backed, certificate-protected key ring before the first payload
can be enqueued. Maintenance reads and publishes only durable UUIDs, so it neither decrypts
protected payloads nor requires the key-ring certificate. A resend atomically
discards older unpublished delivery generations; cancellation and acceptance discard every pending
delivery, and each message carries the invitation expiry as its delivery deadline. Immediately
before sending, the worker must re-read the invitation and conditionally claim the delivery in an
EF-managed transaction, reject discarded or expired messages, and compare the payload secret hash
to the current invitation hash so a delivery claimed during a concurrent state change cannot send a
stale secret. Tampered payloads or payloads protected by an unavailable key are never delivered.
The claim uses a five-minute UUIDv7 processing lease and PostgreSQL `xmin` optimistic concurrency.
It serializes through the organization root before reloading both the outbox row and invitation,
then samples claim time and validates the recipient, role, expiry, and secret as one current
generation. Cap the lease at the invitation delivery deadline and recheck that deadline immediately
before and after the provider call; never start provider submission or mark delivery complete at or
after expiry. Unreadable protected data, unsupported payload versions, invalid envelopes, and data
that cannot be decrypted after key loss receive an explicit durable undeliverable disposition that
recovery excludes, allowing the queue delivery to terminate instead of waking the worker forever.
Cancellation, acceptance, and resend discard every undelivered generation, including one whose
queue hint was already published, and invalidate its processing lease. A handler failure releases
the lease; abandoned work may be reclaimed after lease expiry. Mark the outbox row delivered only
after an in-window email-adapter return. This boundary is at least once: provider acceptance followed
by a failed completion write may submit the same outbox UUID again, so pass that durable UUID to the
provider as its delivery/deduplication identifier whenever supported.

After the transaction commits, the Lambda API uses Rebus sender-only mode to send the outbox UUID to
SQS. This is a low-latency hint, not the sole delivery guarantee. Transport or publication-cursor
failures, a five-second API submission timeout, or request cancellation do not turn the
already-committed business write into an API failure; they leave the row eligible for maintenance
recovery. A timed-out send may still be accepted by the transport, so duplicate hints remain an
intentional part of the at-least-once boundary. Deployments may explicitly set
`Messaging__ApiOutboxForwardingEnabled=false` to run the API in recovery-only mode without queue configuration.

Use two small, versionable Rebus contracts: an organization-invitation delivery UUIDv7 and a
billing-webhook processing UUIDv7. They contain no protected payload, email address, Stripe payload,
or other business data; handlers always load the durable row through the existing application and
EF persistence services. A hint that observes an active processing lease is acknowledged. The
durable maintenance scan republishes it after the lease and publication cursor become stale, which
avoids exhausting immediate broker retries or dead-lettering an otherwise healthy duplicate.

Add a small maintenance Lambda invoked every five minutes. It scans for unpublished or stale outbox
records, sends their UUIDs through Rebus sender-only mode, and emits scheduled reconciliation
commands. This closes the commit/send crash window without requiring a continuously active worker.
Scan at most 100 recoverable records per invocation. Prioritize work that has never been published,
then order by its durable last-publication-or-occurrence cursor, work kind, and UUID. Treat a
publication as stale after five minutes. This moves accepted hints behind still-unpublished work so
the same retrying rows cannot monopolize every batch. Skip expired or discarded outbox messages,
processed billing webhooks, and billing webhooks with an unexpired processing lease. After the
transport accepts each UUID, sample the clock and conditionally record that publication on the
outbox or webhook row, guarded by EF's PostgreSQL `xmin` optimistic-concurrency token. A failed send
stays unpublished, while failure after transport acceptance deliberately permits a duplicate hint;
handlers claim their durable rows idempotently. If a billing worker acquires an unexpired processing
lease between discovery and publication bookkeeping, the bookkeeping write yields to that lease so
it cannot invalidate the worker's concurrency token.

Run the Rebus receiver as an ECS Fargate service backed by SQS. Configure service autoscaling with:

- minimum and desired capacity zero;
- an initially configurable maximum of four tasks;
- a step-scaling alarm that starts one task when at least one message is visible;
- higher queue thresholds that add tasks as backlog grows; and
- scale-in to zero only after both visible and in-flight counts remain zero for ten minutes.

Use a visibility timeout longer than the maximum supported handler duration and graceful shutdown
long enough for Rebus to stop accepting work and finish or abandon messages safely. Alert on old
messages, repeated handler failures, and DLQ entries.

The expected tradeoff is that invitations, billing projections, and reconciliation may take a few
minutes when the worker is cold. This is acceptable initially in exchange for zero idle Fargate
tasks.

In self-hosted/local mode, use the same Rebus message contracts and handlers with RabbitMQ. Ensure
the durable RabbitMQ work and error queues are provisioned before sender-only API or maintenance
processes publish; starting the receiver once declares them and they remain while the worker is
stopped. Keep the work and error queue names distinct so poison messages cannot recycle into the
input queue.
Keep RabbitMQ/SQS selection and API/worker/maintenance process selection in the single executable;
do not split runtime modes into additional projects or add a BFF.

## Email

Use Amazon SES for organization invitations and product-owned notifications. Send email from the
worker with a durable delivery/deduplication identifier. Stripe remains responsible for invoice,
receipt, payment failure, dunning, and other billing emails where it supports the workflow.
Invitation acceptance links must use HTTPS because they carry the protected invitation secret.

Keep email behind an application interface so a later self-hosted deployment can use customer SMTP.
Do not put invitation links or other secrets in application logs.

## Portal

The portal opens directly to Billing. Organization creation and invitations, device details, and
desktop server settings are disclosed on demand; organization membership remains separate from
the license needed to use Styrhous.

The authenticated `GET /api/billing-prices` endpoint reads the configured monthly and annual seat
prices without exposing provider identifiers. It returns cadence, currency, per-seat amount in
display units, and nullable `taxIncluded` (null means tax treatment is confirmed at checkout).
Unsupported price structures or provider failures return `billing_prices_unavailable`; the portal
keeps billing management and checkout available and explicitly defers the price to checkout.

The internal `GET /api/billing-accounts` endpoint gives the authenticated portal a non-cacheable,
provider-safe account portfolio. It returns the user's personal account first, followed by only the
organizations they belong to in deterministic membership order. Each entry contains its local
UUIDv7 identifiers, organization name and current role where applicable, enabled-product-seat count,
the authenticated user's server-resolved seat entitlement, current trial window and activity, and a
bounded subscription summary with status, purchased seat quantity, cancellation state, paid period,
and projection time. The resolved entitlement is authoritative when an inactive, future, or expired
subscription coexists with an active trial. Stripe customer, subscription, and Price identifiers are
never returned or materialized into the listing projection. Every organization member may see this
non-secret billing status, while only the personal owner or current organization Owner is marked as
able to manage billing. Trial summaries retain the natural `endsAt` and expose an optional earlier
`terminatedAt`; entitlement `validUntil` and `isActive` use the effective earlier instant.

For a manageable account without a local commercial-subscription projection, the Billing route
shows the initial-Checkout form. It fixes personal purchases at one seat and initializes organization
purchases to the current enabled-product-seat count, with a minimum of one paid seat. The server
remains authoritative for active seat-bearing invitation reservations and returns its required
minimum when concurrent changes make the draft too small. A
retry after an ambiguous provider failure reuses the returned operation UUID. Editing that live
attempt first clears the portal's local retry identity, but the server returns the one safe live
operation and the portal restores its cadence, quantity, and retry action rather than creating a
second Stripe subscription. Explicit Checkout success and cancellation query states provide return
feedback without treating either browser redirect as entitlement evidence.

For a manageable account with a local subscription projection, the Billing route offers Customer
Portal billing management. It explains that Stripe owns payment methods, invoices, and
cancellation while seat quantity remains in Styrhous. The portal prevents duplicate in-flight
submission, focuses a local alert on provider failure, and allows a direct retry. Non-Owners and
accounts without a subscription never receive that action.

Build the web portal using SvelteKit in static client-only mode. It must include:

- social sign-in and linked-identity management;
- current trial, entitlement, and billing status;
- personal/organization account switching;
- organization creation, roles, members, and invitations;
- seat quantity management;
- Stripe Checkout and Customer Portal redirects;
- desktop device authorization approval;
- per-seat device lists and revocation;
- active browser/device session management; and
- self-hosted license import when running in SelfHosted mode.

Use locally defined design tokens and accessible components. Do not add an external component
framework. Treat the portal and its API as one deployable product that can change together.
Match the desktop application's visual language: a dark navigation surface above a light workspace,
Inter typography, flat white cards, subtle gray borders, compact six-pixel-radius controls, indigo
primary and soft actions, and solid red destructive actions. Reuse the desktop font and icon assets
from their canonical repository locations rather than copying them into the portal.

Keep the portal package, npm lockfile, source, and browser fixtures under `licensing/portal`. Disable
root-layout SSR and use the static adapter's `index.html` fallback so CloudFront and the self-hosted
web server can serve the same client-side routes. The initial shell provides Overview,
Organizations, Devices, and Billing routes with responsive local design tokens. Its CI job runs
Svelte type/component checks, Vitest behavior tests, a production build, and Playwright against
desktop and narrow Chromium viewports. Commit both pixel baselines and accessibility-tree snapshots;
browser tests must also cover keyboard focus order, client-side navigation, and direct deep links.
Keep non-navigated button, text-input, and select showcase routes at the desktop snapshot size of
1536 by 1024 pixels. The showcases must use the portal's production control rules; route-specific
CSS may position controls on the comparison canvas but must not recreate their colors, typography,
padding, focus treatment, or interaction states. Run them in a dedicated Playwright project and
retain corresponding visible text, component geometry, interaction states, pixel snapshots, and
accessibility snapshots so Kineprism can compare the native and browser renderings directly. A
native combobox is compared to the portal's real native select only in its deterministic closed
state; the browser-owned open popup is not treated as a portable visual oracle.

## API stability boundary

The SPA-facing API is internal, unversioned, and intentionally unstable. It may change freely with
the portal. Generated frontend types may be refreshed in the same change, but this surface has no
backward-compatibility gate.

Only the released desktop licensing protocol is stable. Place it under `/desktop/v1` and include:

- device authorization initiation and browser approval;
- token polling, refresh, rotation, and revocation;
- seat selection;
- stable device-limit responses;
- device listing/revocation needed by activation;
- entitlement checks;
- entitlement lease issuance; and
- signing-key discovery.

Implement the OAuth device authorization protocol through OpenIddict with:

- a public desktop client;
- ten-minute device codes;
- a five-second polling interval;
- short-lived access tokens;
- rotating 90-day refresh tokens; and
- refresh-token reuse detection and session revocation.

Publish an OpenAPI document containing only the desktop protocol and keep it under compatibility
tests. Additive optional fields are allowed in v1. A breaking change requires `/desktop/v2` and an
explicit desktop migration strategy.

Issue a signed seven-day entitlement lease containing:

- schema version;
- issuer and desktop audience;
- UUIDv7 user, seat, and device IDs;
- entitlement state and reason;
- source account;
- validity and refresh timestamps;
- token UUIDv7; and
- signing key ID.

Publish rotating verification keys. An expired or invalid lease falls back to evaluation rather
than disabling Styrhous.

## Deployment modes

### SaaS

Use Pulumi in C# with Pulumi Cloud state to provision:

- a private S3 bucket and CloudFront distribution for the static portal;
- same-origin routing to API Gateway and the ASP.NET Core 10 Lambda;
- the scheduled maintenance Lambda;
- the scale-to-zero Fargate worker;
- RDS PostgreSQL with RDS Proxy;
- SQS and its DLQ;
- SES;
- Secrets Manager and signing/data-protection secrets;
- private networking and the outbound access needed for Stripe and social providers; and
- CloudWatch logs, dashboards, and alarms.

Required alarms include API failure rate, database health, Stripe webhook failures, oldest outbox
age, visible/in-flight queue depth, oldest SQS message, Fargate startup/handler failures, and DLQ
messages.

Exact AWS region, domain names, OAuth registrations, Stripe IDs, email domain, prices, and alert
destinations are Pulumi stack configuration rather than source constants.

### SelfHosted

SelfHosted mode uses the same API, portal, domain, persistence, and worker code but:

- disables Stripe and public social-provider requirements;
- uses PostgreSQL and RabbitMQ;
- accepts one customer-configured generic OIDC provider;
- supports a one-time secret-backed bootstrap administrator; and
- imports a vendor-signed ES256 annual license.

The signed license contains a license UUIDv7, customer, product, seat count, default or overridden
device limit, features, validity, grace period, schema version, and signing key ID. The vendor private
key is never deployed to the customer system. Support verification-key rotation and retain an audit
record containing the imported license hash and verified claims.

This milestone proves SelfHosted mode through Docker Compose and automated tests. Production Helm
charts, offline image/archive export, backup and restore, upgrade automation, vendor issuance
tooling, and formal air-gap certification are deferred.

## Nix development environment

Update the existing `flake.nix` and lock only as required instead of adding root-level environment
files.

Add the following development tooling to the shell:

- .NET SDK 10;
- Node.js 22;
- Pulumi; and
- the nixpkgs Playwright browser bundle.

The currently locked nixpkgs provides .NET 10, Node 22, and Playwright driver 1.61.1. Pin the
npm Playwright version to the Nix Playwright driver version. Export
`PLAYWRIGHT_BROWSERS_PATH` to the Nix-provided browser directory and set
`PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1` so npm does not download incompatible dynamically linked
browsers on NixOS.

Non-Nix CI may install its own matching Playwright browsers and system dependencies.

## Testing and acceptance

Develop in vertical, behavior-first slices. Start each slice with a focused failing test, complete
all interdependent code for that slice, then run its full validation once the solution is in a
consistent state.

### Domain and persistence

Test:

- UUIDv7 generation for every owned entity;
- automatic trial creation on signup;
- uniqueness of originated trials;
- one-time trial transfer without resetting dates;
- trial and paid seat capacity;
- deterministic commercial funding priority and over-capacity evaluation;
- role and ownership authorization;
- invitation expiry, matching, reservation, and races;
- entitlement state transitions; and
- self-hosted signature, expiry, grace, and key rotation.

Run persistence integration tests against Docker Compose PostgreSQL. Do not mock EF or the database.
Use isolated databases/schemas and deterministic cleanup so tests remain parallel-safe.

### Device protocol

Test:

- the first three activations for a seat;
- independent limits for the same user in multiple organizations;
- automatic eviction of the least-recently-seen device after seven inactive days;
- refusal to evict recently active devices;
- explicit revocation and resumption of a pending approval;
- concurrent approvals at capacity;
- seat selection with multiple entitlements;
- membership or subscription termination;
- refresh rotation, reuse rejection, and revocation;
- entitlement lease verification and expiry; and
- golden signed leases/public keys shared with the Rust client.

Snapshot and compatibility-test only the `/desktop/v1` OpenAPI document. Internal SPA endpoint
changes need current behavior tests but no compatibility approval.

### Messaging and billing

Run Rebus integration tests against Docker Compose RabbitMQ. Cover transactional outbox behavior,
the API sender-only fast path, a failed send followed by maintenance recovery, duplicate delivery,
worker restart, poison messages, and idempotent side effects.

Treat Stripe and social providers as genuinely external boundaries. Use explicit test
implementations of their application interfaces, captured signed webhook fixtures, and separate
opt-in Stripe test-mode/test-clock smoke tests. Cover duplicate and out-of-order webhooks,
reconciliation, cancellation, payment failure, and proration requests. Initial-Checkout coverage
must include personal quantity enforcement, organization ownership and reservation capacity,
unbounded valid integer quantities, durable retry identity, provider failure after persistence,
strict configured Price selection and redirect destinations, Stripe Tax, operation metadata, and
authoritative paid-projection trial termination.
Customer Portal coverage must include personal and organization-Owner authorization, organization
privacy, subscription absence, no browser-owned Stripe inputs, strict configured customer,
configuration, return URL and redirect validation, provider cancellation/outage behavior,
antiforgery, duplicate browser submission prevention, focused retry errors, and non-Owner UI
suppression.
Seat-quantity coverage must include organization ownership and privacy, personal fixed quantity,
assigned-plus-reserved minimum capacity, unbounded valid integer quantities, durable exact retry,
changed-request recovery, pending decrease and increase reservation behavior, provider
supersession, increase proration and immediate invoicing, no-credit decreases, strict managed-item
validation, request cancellation, antiforgery, duplicate submission prevention, and responsive
focused error and success states.

### Portal and accessibility

Use Vitest for frontend behavior and Playwright for end-to-end browser flows. Cover signup with an
already active trial, organizations, invitations, seat changes, billing pending/error states, device
approval at and below capacity, explicit revocation, and self-hosted license status.

Capture responsive screenshots and accessibility-tree snapshots. Check keyboard navigation, focus
order, labels, error association, and horizontal/vertical alignment. Verify Playwright inside the
Nix shell as well as CI. Compare the 1536 by 1024 component showcases to their native counterparts
with Kineprism, recording raw MAE and inspecting its annotated expected, actual, and diff artifacts.
Keep the checked source pairs, occupied-control crop bounds, and latest reviewed metrics in
`portal/e2e/component-showcase.comparisons.json`. Bind each reviewed metric to SHA-256 hashes of its
native and browser PNGs so either snapshot changing requires a fresh Kineprism review. Treat those
crop metrics as primary for sparse control canvases; full-canvas metrics are supporting context
because transparent space dilutes them. Compare the opaque occupied rectangle shared by each native
and browser fixture so alpha-only canvas differences cannot dominate the score. Each pair records a
tight reviewed maximum raw MAE, every maximum must remain at or below 0.06, and the Playwright
manifest test must reject both an exceeded maximum and more than 0.002 of unreviewed headroom.
Kineprism may still report individual pairs as non-equivalent because Chromium and egui rasterize
the same Inter glyphs differently; the bounded occupied-region score, exact browser snapshot, source
hashes, and inspected artifacts together form the regression gate.

### Infrastructure

Validate Pulumi through tests and staging previews. Smoke-test:

- static SPA and API routing;
- OAuth callbacks and shared cookie protection;
- Lambda-to-RDS connectivity;
- Stripe webhook ingestion;
- the maintenance outbox recovery path with no worker running;
- Fargate scale from zero when SQS receives work;
- no scale-in while messages remain in flight;
- return to zero after sustained idle time;
- SES sandbox delivery; and
- DLQ and operational alarms.

CI should run backend formatting/build/tests, frontend checks/tests, Compose integration tests,
desktop protocol compatibility, and Pulumi validation independently of the existing Rust Nextest
jobs.

## Post-change review

After each substantial completed changeset, run the required independent reviews in parallel:

1. Critical correctness review focused on bugs, races, security, and edge cases.
2. Duplication and abstraction review focused on reusable behavior and cleaner language patterns.
3. Test review focused on missing behavior, integration scenarios, and useful snapshots.
4. Accessibility review for changed portal screenshots and accessibility snapshots, including
   alignment and grouping.

Resolve findings before final validation and handoff.

## Deferred work and assumptions

- Updater and release-download gating remain outside the first licensing milestone.
- Exact prices, annual discount, branding copy, domains, AWS region, provider registrations, legal
  documents, and privacy/terms text are deployment or business inputs.
- Commercial terms acceptance should be versioned and recorded at checkout, but drafting legal
  terms is not part of the implementation.
- Licensing remains honor-based and is not hardware-machine-locked beyond active-device limits.
- Initial asynchronous latency of a few minutes is acceptable in exchange for zero idle Fargate
  worker tasks.
- Production self-hosted packaging, offline delivery, installation automation, backup/restore,
  upgrades, and license issuance operations are later milestones.
