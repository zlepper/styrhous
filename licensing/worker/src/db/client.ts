import { drizzle } from 'drizzle-orm/d1';
import type { BatchItem } from 'drizzle-orm/batch';
import { getTableColumns, sql, type SQL } from 'drizzle-orm';
import type { SQLiteTable } from 'drizzle-orm/sqlite-core';
import type { MiddlewareHandler } from 'hono';
import type { AppEnv, Database } from '../env';
import * as schema from './schema';

export function createDatabase(d1: D1Database): Database {
  return drizzle(d1, { schema });
}

export const database: MiddlewareHandler<AppEnv> = async (c, next) => {
  c.set('db', createDatabase(c.env.DB));
  await next();
};

/** Runs a batch assembled at runtime; D1 commits all statements atomically, in order. */
export async function runBatch(
  db: Database,
  statements: readonly BatchItem<'sqlite'>[]
): Promise<D1Result[]> {
  const [first, ...rest] = statements;
  if (!first) return [];
  return (await db.batch([first, ...rest])) as D1Result[];
}

/**
 * Inserts one row only when `guard` holds at write time (`INSERT … SELECT … WHERE guard`), so
 * an invariant check and the write it protects are a single statement.
 */
export function insertWhere<Table extends SQLiteTable>(
  db: Database,
  table: Table,
  values: Table['$inferInsert'],
  guard: SQL
) {
  const row = Object.entries(getTableColumns(table)).map(([key, column]) => {
    const value = (values as Record<string, unknown>)[key] ?? (column.hasDefault ? column.default : null);
    return value === null || value === undefined ? sql`NULL` : sql`${column.mapToDriverValue(value)}`;
  });
  return db.insert(table).select(sql`SELECT ${sql.join(row, sql`, `)} WHERE ${guard}`);
}
