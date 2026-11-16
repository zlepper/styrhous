import { sql } from 'drizzle-orm';
import type { Database } from './env';
import { auditRecords } from './db/schema';
import { insertWhere } from './db/client';
import { uuidv7 } from './lib/ids';

export type AuditEntry = Readonly<{
  correlationId: string;
  actorUserId: string | null;
  action: string;
  targetType: string;
  targetId: string;
  occurredAt: string;
}>;

/**
 * Builds an audit insert for a batch. Place it directly after the statement it records: it
 * only writes when that statement changed rows, so rejected guarded writes leave no record.
 */
export function auditInsert(db: Database, entry: AuditEntry) {
  return insertWhere(db, auditRecords, { id: uuidv7(), ...entry }, sql`changes() > 0`);
}

/** Audit records for one request, sharing its actor, time, and correlation id. */
export function auditor(db: Database, actor: Readonly<{ actorUserId: string | null; occurredAt: string }>) {
  const correlationId = uuidv7();
  const entry = (action: string, targetType: string, targetId: string): AuditEntry => ({
    correlationId,
    ...actor,
    action,
    targetType,
    targetId
  });
  return {
    correlationId,
    /** A batch statement for {@link auditInsert}. */
    insert: (action: string, targetType: string, targetId: string) =>
      auditInsert(db, entry(action, targetType, targetId)),
    /** Records a change that has already been committed. */
    async record(action: string, targetType: string, targetId: string) {
      await db.insert(auditRecords).values({ id: uuidv7(), ...entry(action, targetType, targetId) });
    }
  };
}
