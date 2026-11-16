# Desktop licensing protocol v2

The Styrhous desktop app talks to its licensing server over `/desktop/v2`. The server origin
(`https://host`, no path) is also the expected lease issuer, so every URL below is relative to
that origin. Responses are JSON and never cached.

Licensing is honor-based: without a valid lease the app shows evaluation status, but it never
disables features.

## Sign-in (RFC 8628 device authorization)

### `POST /desktop/v2/device/authorize`

Form-encoded installation metadata:

| Field | Limits |
|---|---|
| `installation_id` | UUID, stable per installation |
| `display_name` | 1–120 characters |
| `platform` | 1–64 characters |
| `architecture` | 1–32 characters |
| `styrhous_version` | 1–64 characters |

`200`:

```json
{
  "device_code": "<secret>",
  "user_code": "BCDF-GHJK",
  "verification_uri": "https://host/devices/authorize",
  "verification_uri_complete": "https://host/devices/authorize?user_code=BCDF-GHJK",
  "expires_in": 600,
  "interval": 5
}
```

The client opens `verification_uri_complete` in the browser. Both verification URLs are always
on the server origin, and clients must reject any other origin. `400 {"error":"invalid_request"}`
means the metadata was invalid.

### `POST /desktop/v2/token`

Form field `device_code`. The client waits `interval` seconds before each poll.

| Response | Meaning |
|---|---|
| `200 {"installation_token": "<secret>"}` | Approved. Store the token in the OS keychain. |
| `400 {"error":"authorization_pending"}` | Keep polling. |
| `400 {"error":"slow_down"}` | Add 5 seconds to the interval. |
| `400 {"error":"access_denied"}` | The user denied the request. |
| `400 {"error":"expired_token"}` | The code expired; start again. |
| `400 {"error":"invalid_grant"}` | Unknown or already used code. |

The installation token is a long-lived bearer credential for one device activation. It stays
valid until the device is signed out, revoked from the portal, removed with its organization
membership, or replaced as a stale device.

## Licensed use

### `POST /desktop/v2/entitlement`

`Authorization: Bearer <installation_token>`, no body.

| Response | Client behaviour |
|---|---|
| `200 {"lease", "state", "entitlementReasonCode", "expiresAt", "refreshAfter"}` | Fetch `/keys`, verify the lease, and cache both. |
| `401 {"reasonCode":"invalid_installation_token"}` | Forget the token; signed out. |
| `403 {"reasonCode":"<entitlement reason>"}` | Evaluation mode. Stay signed in and check again later. |

### `GET /desktop/v2/keys`

A JWKS of Ed25519 verification keys (`kty: OKP`, `crv: Ed25519`, `alg: EdDSA`, `use: sig`,
`kid`). The current key comes first; retired keys stay listed while cached leases may use them.

### `POST /desktop/v2/sign-out`

`Authorization: Bearer <installation_token>`. Always `204`; revokes the device activation.

## Offline lease

The lease is a compact JWS with header `{"alg":"EdDSA","typ":"JWT","kid":"<kid>"}` and these
claims:

| Claim | Value |
|---|---|
| `schema_version` | `2` |
| `iss` | Server origin with a trailing slash, e.g. `https://styrhous.app/` |
| `aud` | `styrhous-desktop` |
| `sub`, `seat_id`, `billing_account_id`, `installation_id`, `activation_id` | UUIDs |
| `state` | `trial`, `commercial`, or `grace` |
| `reason_code` | Entitlement reason, e.g. `active_subscription` |
| `valid_from`, `valid_until` | Optional ISO-8601 entitlement window (informational) |
| `iat` = `nbf` | Issue time (Unix seconds) |
| `exp` | The earlier of `iat` + 7 days and the entitlement end |
| `refresh_after` | The earlier of `iat` + 24 hours and `exp` |
| `jti` | UUID (any version) |
| `signing_key_id` | Equals the header `kid` |

Clients verify the signature with the matching `kid` from `/keys`. They require every claim
above and check:

- `installation_id` matches the local installation.
- `iat ≤ nbf ≤ now + 5 minutes`: a lease may start up to five minutes ahead of the local clock,
  so a desktop clock that lags the server still accepts a freshly issued lease.
- `now < exp`, with no leeway, and `exp − iat ≤ 7 days`. A local clock that runs ahead only makes
  the lease end early, and the client refreshes it well before then (`refresh_after`).
- `iat ≤ refresh_after ≤ exp`.

A lease that fails any check is discarded.

`licensing/protocol-fixtures/` holds golden leases signed with a test key. The Worker test suite
reproduces them byte for byte and the Rust client verifies them, so both sides stay in step.
