import { describe, expect, it } from 'vitest';
import { isCurrentPortalRoute, portalNavigation } from './navigation';

describe('portal navigation', () => {
  it('keeps the primary account sections in a stable, unique order', () => {
    expect(portalNavigation.map((item) => item.label)).toEqual([
      'Organizations',
      'Devices',
      'Billing',
      'Account'
    ]);
    expect(new Set(portalNavigation.map((item) => item.href)).size).toBe(portalNavigation.length);
  });

  it.each([
    ['/', '/', true],
    ['/organizations', '/', false],
    ['/organizations', '/organizations', true],
    ['/organizations/active', '/organizations', true],
    ['/devices', '/organizations', false]
  ] as const)('matches %s against %s', (pathname, href, expected) => {
    expect(isCurrentPortalRoute(pathname, href)).toBe(expected);
  });
});
