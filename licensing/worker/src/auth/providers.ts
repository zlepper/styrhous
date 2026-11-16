import { GitHub, Google, MicrosoftEntraId, decodeIdToken } from 'arctic';
import { isUuid } from '../lib/ids';

export const providerNames = ['github', 'google', 'microsoft'] as const;

export type ProviderName = (typeof providerNames)[number];

/** A verified identity from an external provider. */
export type ExternalIdentity = Readonly<{ subject: string; email: string }>;

/** A sign-in failure reported back to the portal as `?authenticationError=<reasonCode>`. */
export class AuthenticationFailure extends Error {
  constructor(readonly reasonCode: string) {
    super(reasonCode);
    this.name = 'AuthenticationFailure';
  }
}

type Provider = Readonly<{
  authorizationUrl(state: string, codeVerifier: string): URL;
  identify(code: string, codeVerifier: string): Promise<ExternalIdentity>;
}>;

export function isProviderName(value: string): value is ProviderName {
  return (providerNames as readonly string[]).includes(value);
}

export function configuredProviders(env: Env): ProviderName[] {
  return providerNames.filter((name) => credentials(env, name) !== null);
}

export function provider(env: Env, name: ProviderName): Provider | null {
  const configured = credentials(env, name);
  if (!configured) return null;
  const { clientId, clientSecret } = configured;
  const redirectUri = `${env.PUBLIC_ORIGIN}/auth/callback/${name}`;
  switch (name) {
    case 'github':
      return gitHubProvider(new GitHub(clientId, clientSecret, redirectUri));
    case 'google':
      return googleProvider(new Google(clientId, clientSecret, redirectUri));
    case 'microsoft':
      return microsoftProvider(
        new MicrosoftEntraId('common', clientId, clientSecret, redirectUri)
      );
  }
}

function credentials(env: Env, name: ProviderName) {
  const [clientId, clientSecret] = {
    github: [env.GITHUB_CLIENT_ID, env.GITHUB_CLIENT_SECRET],
    google: [env.GOOGLE_CLIENT_ID, env.GOOGLE_CLIENT_SECRET],
    microsoft: [env.MICROSOFT_CLIENT_ID, env.MICROSOFT_CLIENT_SECRET]
  }[name];
  return clientId && clientSecret ? { clientId, clientSecret } : null;
}

function gitHubProvider(client: GitHub): Provider {
  return {
    authorizationUrl: (state) => client.createAuthorizationURL(state, ['user:email']),
    async identify(code) {
      const accessToken = (await client.validateAuthorizationCode(code)).accessToken();
      const user = await gitHubApi<{ id?: unknown }>('/user', accessToken);
      const emails = await gitHubApi<unknown>('/user/emails', accessToken);
      if (typeof user.id !== 'number') throw new AuthenticationFailure('invalid_external_identity');
      return identity(String(user.id), gitHubVerifiedEmail(emails), 'verified_email_required');
    }
  };
}

async function gitHubApi<T>(path: string, accessToken: string): Promise<T> {
  const response = await fetch(`https://api.github.com${path}`, {
    headers: {
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${accessToken}`,
      'User-Agent': 'styrhous-licensing'
    }
  });
  if (!response.ok) throw new AuthenticationFailure('provider_authentication_failed');
  return (await response.json()) as T;
}

type GitHubEmail = { email?: unknown; verified?: unknown; primary?: unknown };

/** The primary verified address, otherwise any verified address. */
function gitHubVerifiedEmail(emails: unknown): string | null {
  if (!Array.isArray(emails)) return null;
  const verified = (emails as GitHubEmail[]).filter(
    (item) => item.verified === true && typeof item.email === 'string'
  );
  const chosen = verified.find((item) => item.primary === true) ?? verified[0];
  return (chosen?.email as string | undefined) ?? null;
}

function googleProvider(client: Google): Provider {
  return {
    authorizationUrl: (state, verifier) =>
      client.createAuthorizationURL(state, verifier, ['openid', 'email']),
    async identify(code, verifier) {
      // The ID token comes straight from Google's token endpoint over TLS.
      const tokens = await client.validateAuthorizationCode(code, verifier);
      const claims = decodeIdToken(tokens.idToken()) as Record<string, unknown>;
      if (typeof claims.sub !== 'string') throw new AuthenticationFailure('invalid_external_identity');
      const email =
        claims.email_verified === true && typeof claims.email === 'string' ? claims.email : null;
      return identity(claims.sub, email, 'verified_email_required');
    }
  };
}

function microsoftProvider(client: MicrosoftEntraId): Provider {
  return {
    authorizationUrl: (state, verifier) =>
      client.createAuthorizationURL(state, verifier, ['openid', 'email', 'profile']),
    async identify(code, verifier) {
      const tokens = await client.validateAuthorizationCode(code, verifier);
      const claims = decodeIdToken(tokens.idToken()) as Record<string, unknown>;
      const { tid, oid, iss } = claims;
      if (
        typeof tid !== 'string' ||
        !isUuid(tid) ||
        typeof oid !== 'string' ||
        iss !== `https://login.microsoftonline.com/${tid}/v2.0`
      ) {
        throw new AuthenticationFailure('invalid_external_identity');
      }
      return identity(
        `${tid}:${oid}`,
        microsoftVerifiedEmail(claims),
        'microsoft_verified_email_required'
      );
    }
  };
}

/**
 * Entra only vouches for `verified_primary_email`/`verified_secondary_email`, or for `email`
 * when the domain-owner-verified flag `xms_edov` is set.
 */
function microsoftVerifiedEmail(claims: Record<string, unknown>): string | null {
  const authoritative = [claims.verified_primary_email, claims.verified_secondary_email]
    .flatMap((value) => (Array.isArray(value) ? value : [value]))
    .find((value): value is string => typeof value === 'string' && value.trim() !== '');
  if (authoritative) return authoritative.trim();
  return claims.xms_edov === true && typeof claims.email === 'string' ? claims.email.trim() : null;
}

function identity(
  subject: string,
  email: string | null,
  missingEmailReason: string
): ExternalIdentity {
  if (subject.length === 0 || subject.length > 512) {
    throw new AuthenticationFailure('invalid_external_identity');
  }
  const trimmed = email?.trim();
  if (!trimmed || trimmed.length > 320) throw new AuthenticationFailure(missingEmailReason);
  return { subject, email: trimmed };
}
