import { SignJWT, calculateJwkThumbprint, importJWK, type CryptoKey, type JWK } from 'jose';
import { uuidv7 } from '../lib/ids';
import { dayMs, earliest, hourMs, isoAfter } from '../lib/time';
import type { SeatEntitlement } from '../entitlements/rules';

export const leaseAudience = 'styrhous-desktop';
const leaseSchemaVersion = 2;
const leaseAlgorithm = 'EdDSA';
const maxLeaseMs = 7 * dayMs;
const refreshAfterMs = 24 * hourMs;

type SigningKey = Readonly<{ key: CryptoKey; publicJwk: JWK & { kid: string } }>;

let cachedKey: { source: string; key: Promise<SigningKey> } | null = null;

function signingKey(env: Env): Promise<SigningKey> {
  if (cachedKey?.source !== env.LEASE_SIGNING_KEY) {
    cachedKey = { source: env.LEASE_SIGNING_KEY, key: loadSigningKey(env.LEASE_SIGNING_KEY) };
  }
  return cachedKey.key;
}

async function loadSigningKey(source: string): Promise<SigningKey> {
  const privateJwk = JSON.parse(source) as JWK;
  if (privateJwk.kty !== 'OKP' || privateJwk.crv !== 'Ed25519' || !privateJwk.d) {
    throw new Error('LEASE_SIGNING_KEY must be a private Ed25519 JWK.');
  }
  return {
    key: (await importJWK(privateJwk, leaseAlgorithm)) as CryptoKey,
    publicJwk: await publicJwk({ kty: 'OKP', crv: 'Ed25519', x: privateJwk.x })
  };
}

async function publicJwk(jwk: JWK): Promise<JWK & { kid: string }> {
  const { kty, crv, x } = jwk;
  const base = { kty, crv, x };
  return { ...base, use: 'sig', alg: leaseAlgorithm, kid: jwk.kid ?? (await calculateJwkThumbprint(base)) };
}

/** The current verification key first, then retired keys that may still verify cached leases. */
export async function leaseKeys(env: Env): Promise<{ keys: JWK[] }> {
  const current = (await signingKey(env)).publicJwk;
  const previous = JSON.parse(env.LEASE_PREVIOUS_PUBLIC_KEYS || '[]') as JWK[];
  return { keys: [current, ...(await Promise.all(previous.map(publicJwk)))] };
}

export type LeaseSubject = Readonly<{
  userId: string;
  installationId: string;
  activationId: string;
  entitlement: SeatEntitlement;
}>;

export type SignedLease = Readonly<{ lease: string; expiresAt: string; refreshAfter: string }>;

/** Fixed issue time and token id, used to produce deterministic protocol fixtures. */
export type LeaseOptions = Readonly<{ now?: Date; jti?: string }>;

/** Signs an offline lease that the desktop app verifies against `/desktop/v2/keys`. */
export async function signLease(env: Env, subject: LeaseSubject, options: LeaseOptions = {}): Promise<SignedLease> {
  const { key, publicJwk: jwk } = await signingKey(env);
  const issuedAt = (options.now ?? new Date()).toISOString();
  const { entitlement } = subject;
  const maxExpiry = isoAfter(issuedAt, maxLeaseMs);
  const expiresAt = entitlement.validUntil ? earliest(maxExpiry, entitlement.validUntil) : maxExpiry;
  const refreshAfter = earliest(isoAfter(issuedAt, refreshAfterMs), expiresAt);
  const seconds = (iso: string) => Math.floor(new Date(iso).getTime() / 1000);

  const lease = await new SignJWT({
    schema_version: leaseSchemaVersion,
    seat_id: entitlement.seatId,
    billing_account_id: entitlement.billingAccountId,
    installation_id: subject.installationId,
    activation_id: subject.activationId,
    state: entitlement.state,
    reason_code: entitlement.reasonCode,
    ...(entitlement.validFrom ? { valid_from: entitlement.validFrom } : {}),
    ...(entitlement.validUntil ? { valid_until: entitlement.validUntil } : {}),
    refresh_after: seconds(refreshAfter),
    signing_key_id: jwk.kid
  })
    .setProtectedHeader({ alg: leaseAlgorithm, typ: 'JWT', kid: jwk.kid })
    .setIssuer(`${env.PUBLIC_ORIGIN}/`)
    .setAudience(leaseAudience)
    .setSubject(subject.userId)
    .setIssuedAt(seconds(issuedAt))
    .setNotBefore(seconds(issuedAt))
    .setExpirationTime(seconds(expiresAt))
    .setJti(options.jti ?? uuidv7())
    .sign(key);
  return { lease, expiresAt, refreshAfter };
}
