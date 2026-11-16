import { and, eq, gt } from 'drizzle-orm';
import type { Context, MiddlewareHandler } from 'hono';
import { deleteCookie, getCookie, setCookie } from 'hono/cookie';
import type { AppEnv, Database, Session } from '../env';
import { runBatch } from '../db/client';
import { sessions, users } from '../db/schema';
import { fail } from '../http/errors';
import { randomToken, sha256Hex, uuidv7 } from '../lib/ids';
import { dayMs, isoAfter, isoNow, minuteMs } from '../lib/time';

const sessionCookie = '__Host-styrhous-session';
const sessionLifetimeMs = 30 * dayMs;
const recentAuthenticationMs = 10 * minuteMs;

export const loadSession: MiddlewareHandler<AppEnv> = async (c, next) => {
  c.set('session', await findSession(c.var.db, getCookie(c, sessionCookie)));
  await next();
};

async function findSession(db: Database, token: string | undefined): Promise<Session | null> {
  if (!token) return null;
  const [row] = await db
    .select({
      id: sessions.id,
      userId: sessions.userId,
      email: users.email,
      provider: sessions.provider,
      authTime: sessions.authTime
    })
    .from(sessions)
    .innerJoin(users, eq(users.id, sessions.userId))
    .where(and(eq(sessions.tokenHash, await sha256Hex(token)), gt(sessions.expiresAt, isoNow())));
  return row ?? null;
}

/** The signed-in session, or a 401 for anonymous requests. */
export function requireSession(c: Context<AppEnv>): Session {
  return c.var.session ?? fail(401, 'authentication_required');
}

export function isRecentlyAuthenticated(session: Session): boolean {
  return Date.now() - new Date(session.authTime).getTime() < recentAuthenticationMs;
}

/** Signs the browser in, replacing any session it already had. */
export async function startSession(c: Context<AppEnv>, userId: string, provider: string) {
  const db = c.var.db;
  const token = randomToken();
  const now = isoNow();
  const previous = c.var.session;
  await runBatch(db, [
    ...(previous ? [db.delete(sessions).where(eq(sessions.id, previous.id))] : []),
    db.insert(sessions).values({
      id: uuidv7(),
      tokenHash: await sha256Hex(token),
      userId,
      provider,
      authTime: now,
      createdAt: now,
      expiresAt: isoAfter(now, sessionLifetimeMs)
    })
  ]);
  setCookie(c, sessionCookie, token, {
    httpOnly: true,
    secure: true,
    sameSite: 'Lax',
    path: '/',
    maxAge: sessionLifetimeMs / 1000
  });
}

export async function endSession(c: Context<AppEnv>, session: Session) {
  await c.var.db.delete(sessions).where(eq(sessions.id, session.id));
  deleteCookie(c, sessionCookie, { secure: true, path: '/' });
}

export async function refreshAuthenticationTime(db: Database, session: Session, provider: string) {
  await db
    .update(sessions)
    .set({ authTime: isoNow(), provider })
    .where(eq(sessions.id, session.id));
}
