import type { MiddlewareHandler } from 'hono';
import type { AppEnv } from '../env';
import { fail } from './errors';

const safeMethods = new Set(['GET', 'HEAD', 'OPTIONS']);

/** Browser hardening headers for every response, including portal assets. */
export const securityHeaders: MiddlewareHandler<AppEnv> = async (c, next) => {
  await next();
  c.header('Content-Security-Policy', "frame-ancestors 'none'");
  c.header('Referrer-Policy', 'strict-origin-when-cross-origin');
  c.header('X-Content-Type-Options', 'nosniff');
  c.header('X-Frame-Options', 'DENY');
};

/** API, authentication, and desktop responses are never cached. */
export const noStore: MiddlewareHandler<AppEnv> = async (c, next) => {
  await next();
  c.header('Cache-Control', 'no-store');
};

/**
 * Cookie-authenticated mutations must come from the portal's own origin. Browsers always
 * send `Origin` on non-GET fetches, so a missing or foreign origin is rejected.
 */
export const sameOriginMutations: MiddlewareHandler<AppEnv> = async (c, next) => {
  if (!safeMethods.has(c.req.method) && c.req.header('Origin') !== c.env.PUBLIC_ORIGIN) {
    fail(403, 'cross_origin_request_rejected');
  }
  await next();
};
