# Styrhous licensing

Styrhous licensing is an honor-based commercial signal. Personal use, education, non-commercial
open source, and organizations with fewer than 250 people and less than EUR 10M in annual revenue
are free and need no account (see `LICENSE.md`). Larger organizations buy recurring per-seat
Stripe subscriptions:

- Signing in creates a personal account and seat without a trial.
- The first organization a user creates starts a 30-day trial with 5 seats. Each user starts at
  most one trial.
- Organizations have Owner, Admin, and Member roles. Product seats are assigned separately from
  roles.
- A seat allows three active desktop devices. A device unseen for seven days may be replaced.

The whole service is **one Cloudflare Worker** (`worker/`):

```text
browser portal ─┐                       ┌─ D1 (all state)
desktop app ────┼──> Worker (Hono) ─────┼─ Email Service (invitations)
Stripe webhooks ┘     │                 └─ Stripe / GitHub / Google / Microsoft APIs
                      └─ ASSETS: static SvelteKit portal (portal/build)
```

- `/api/*` and `/auth/*` serve the portal. They use a D1-backed session cookie, and mutations
  must carry the portal's `Origin`.
- `/desktop/v2/*` is the desktop protocol; see [desktop-protocol.md](desktop-protocol.md).
- `/api/webhooks/stripe` takes signed Stripe events. They are processed inline by re-reading the
  subscription from Stripe, and Stripe retries any failure.
- Every other path is the portal, with an SPA fallback to `index.html`.
- A daily cron removes expired sessions, sign-in states, device authorizations, and old
  invitations.

## Local development

```bash
cd licensing/worker
npm ci
cp .dev.vars.example .dev.vars   # fill in test-mode values
npm run lease-key                # paste privateJwk into LEASE_SIGNING_KEY
npx wrangler d1 migrations apply licensing --local
npm run dev                      # builds the portal, serves everything on http://localhost:5173
```

Debug builds of the desktop app accept a `http://localhost` licensing server. Forward Stripe test
webhooks with `stripe listen --forward-to localhost:5173/api/webhooks/stripe`.

## Validation

```bash
(cd licensing/worker && npm run check && npm test)
(cd licensing/portal && npm test)
cargo nextest run -p styrhous licensing
```

Worker tests run inside workerd against a real local D1. Only third-party HTTP services (Stripe
and OAuth providers) and the email binding are faked.

## Production setup

Production runs at `https://styrhous.app`. Setup happens once, in this order:

1. **D1**: `npx wrangler d1 create licensing`, then put the returned `database_id` in
   `worker/wrangler.jsonc`.
2. **Domain**: the zone must be on Cloudflare. `wrangler.jsonc` routes `styrhous.app` to the Worker
   as a custom domain.
3. **Email**: enable Cloudflare Email Service sending for `styrhous.app` and verify the
   `INVITATION_FROM_ADDRESS` sender domain.
4. **Signing key**: `npm run lease-key`, then `npx wrangler secret put LEASE_SIGNING_KEY` with the
   `privateJwk`. When rotating, append the old `publicJwk` to the `LEASE_PREVIOUS_PUBLIC_KEYS`
   secret (a JSON array) so cached leases keep verifying for up to seven days.
5. **OAuth providers**: register the callback `https://styrhous.app/auth/callback/<provider>` and
   set `GITHUB_CLIENT_ID`/`GITHUB_CLIENT_SECRET` (and likewise `GOOGLE_*`, `MICROSOFT_*`) as
   secrets. A provider is enabled only when both of its values are set. Microsoft needs the
   `email` and `xms_edov` optional ID-token claims.
6. **Stripe**: Checkout uses [Managed Payments](https://docs.stripe.com/payments/managed-payments),
   so Stripe (through Link) is the merchant of record and handles sales tax/VAT, fraud, disputes,
   receipts, and payment support. Stripe Tax registrations are not needed.
   - Enable Managed Payments under *Settings → Managed Payments*, and set the support email under
     *Settings → Business details*.
   - Give the product a Managed Payments-eligible software tax code, with monthly and annual
     per-seat prices.
   - In *Settings → Checkout*, link the terms of service and privacy policy:
     `https://github.com/zlepper/styrhous/blob/master/legal/TERMS_OF_SERVICE.md` and
     `https://github.com/zlepper/styrhous/blob/master/legal/PRIVACY_POLICY.md`.
   - Create a customer portal configuration that allows payment-method updates and cancellation,
     but not quantity changes; seat changes go through the portal so seat capacity is enforced.
     Renewals charge the card stored on the subscription, not the customer's default card. Under
     Managed Payments the API cannot change it, so customers change it with the edit button next
     to the card shown on the subscription in the customer portal.
   - Leave Smart Retries and failed-payment emails enabled under *Billing → Revenue recovery*.
   - Set `STRIPE_SECRET_KEY` and `STRIPE_WEBHOOK_SECRET` as secrets, and the monthly/annual price
     ids and customer portal configuration id as `vars`.
   - Create a webhook endpoint for `https://styrhous.app/api/webhooks/stripe` with
     `checkout.session.completed`, `checkout.session.async_payment_succeeded|failed`,
     `customer.subscription.created|updated|deleted`, `invoice.paid`, and
     `invoice.payment_failed`.
7. **Deployments**: connect the repository in *Workers & Pages → styrhous-licensing → Settings →
   Builds*:
   - root directory `licensing/worker`
   - build watch paths `licensing/**`
   - build command `npm run build`
   - deploy command `npm run deploy`, which applies D1 migrations and then deploys
   - production branch `master`
   - non-production branch builds disabled
   - If the build token lacks D1 edit permission, replace it with one that has *D1 Edit* and
     *Workers Scripts Edit*.
   - If the repository is missing from the list, the Cloudflare GitHub App installation may be
     linked to another Cloudflare account. Open *Configure* for it on GitHub and save a change to
     link it to this account.

8. **Rate limiting** (follow-up): `POST /desktop/v2/device/authorize` and `GET /auth/sign-in/*`
   create rows without authentication, and only the daily cron removes them. Add a Cloudflare
   rate limiting rule for those paths on the `styrhous.app` zone.

Workers Builds deploys independently of GitHub Actions, which only validate. The pre-commit hook
runs the same checks locally.

### Tooling note

The Worker is built with `@cloudflare/vite-plugin` and deployed with Wrangler. Cloudflare's newer
`cf` CLI builds on the same Vite plugin; switch once it is out of beta (`cf migrate`).
