/** Whether a D1 failure (possibly wrapped by drizzle) is a UNIQUE constraint violation. */
export function isUniqueViolation(error: unknown): boolean {
  for (let current: unknown = error; current instanceof Error; current = current.cause) {
    if (current.message.includes('UNIQUE constraint failed')) return true;
  }
  return false;
}
