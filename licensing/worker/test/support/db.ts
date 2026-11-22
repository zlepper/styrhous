import { env } from 'cloudflare:workers';

/** A timestamp long in the past, for expiring records directly in D1. */
export const past = '2020-01-01T00:00:00.000Z';

export async function countRows(query: string, ...binds: unknown[]): Promise<number> {
  const row = await env.DB.prepare(query).bind(...binds).first<{ total: number }>();
  return row!.total;
}

/** Ends the trial that a user started with their first organization. */
export async function expireUserTrial(userId: string) {
  await env.DB.prepare('UPDATE trials SET started_at = ?, ends_at = ? WHERE originating_user_id = ?')
    .bind(past, '2020-01-31T00:00:00.000Z', userId)
    .run();
}
