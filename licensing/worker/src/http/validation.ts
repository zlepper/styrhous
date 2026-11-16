import { zValidator } from '@hono/zod-validator';
import type { ValidationTargets } from 'hono';
import type { ZodType } from 'zod';

/**
 * Validates a request part and reports failures in the problem-details shape the portal
 * understands: `{ title, errors: { field: [message] } }`.
 */
export function validate<Target extends keyof ValidationTargets, Schema extends ZodType>(
  target: Target,
  schema: Schema
) {
  return zValidator(target, schema, (result, c) => {
    if (result.success) return;
    const errors: Record<string, string[]> = {};
    for (const issue of result.error.issues) {
      const field = issue.path.map(String).join('.') || target;
      (errors[field] ??= []).push(issue.message);
    }
    return c.json({ title: 'One or more validation errors occurred.', errors }, 400);
  });
}
