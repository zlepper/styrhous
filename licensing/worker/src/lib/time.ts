export const minuteMs = 60_000;
export const hourMs = 60 * minuteMs;
export const dayMs = 24 * hourMs;

export function isoNow(): string {
  return new Date().toISOString();
}

export function isoAfter(start: string | number, durationMs: number): string {
  return new Date(new Date(start).getTime() + durationMs).toISOString();
}

export function earliest(first: string, second: string): string {
  return first < second ? first : second;
}
