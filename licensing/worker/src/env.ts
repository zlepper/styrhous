import type { DrizzleD1Database } from 'drizzle-orm/d1';
import type * as schema from './db/schema';

/** Optional secrets: a sign-in provider is enabled only when both of its values are set. */
interface OptionalSecrets {
  GITHUB_CLIENT_ID?: string;
  GITHUB_CLIENT_SECRET?: string;
  GOOGLE_CLIENT_ID?: string;
  GOOGLE_CLIENT_SECRET?: string;
  MICROSOFT_CLIENT_ID?: string;
  MICROSOFT_CLIENT_SECRET?: string;
  /** JSON array of retired public JWKs that may still verify cached leases. */
  LEASE_PREVIOUS_PUBLIC_KEYS?: string;
}

declare global {
  interface Env extends OptionalSecrets {}
  namespace Cloudflare {
    interface Env extends OptionalSecrets {}
  }
}

export type Database = DrizzleD1Database<typeof schema>;

export type Session = Readonly<{
  id: string;
  userId: string;
  email: string;
  provider: string;
  authTime: string;
}>;

export type AppEnv = {
  Bindings: Env;
  Variables: {
    db: Database;
    session: Session | null;
  };
};
