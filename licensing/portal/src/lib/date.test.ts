import { describe, expect, it } from 'vitest';
import { formatUtcDateTime } from './date';

describe('UTC date formatting', () => {
  it('shows equivalent instants consistently regardless of their source offset', () => {
    expect(formatUtcDateTime('2026-09-01T12:15:00+02:00')).toBe(
      formatUtcDateTime('2026-09-01T10:15:00Z')
    );
  });
});
