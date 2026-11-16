import { exports } from 'cloudflare:workers';
import { beforeEach, describe, expect, it } from 'vitest';
import { signedInBrowser } from './support/identity.ts';
import { Browser, origin } from './support/portal.ts';
import { FakeUpstream } from './support/upstream.ts';

let upstream: FakeUpstream;

beforeEach(() => {
  upstream = new FakeUpstream().install();
});

describe('hosting', () => {
  it('serves the portal for client routes and JSON 404s for unknown backend paths', async () => {
    const browser = new Browser();
    for (const path of ['/billing', '/devices/authorize?user_code=BCDF-GHJK', '/invitations/accept', '/apiary']) {
      const page = await browser.fetch(path);
      expect(page.status, path).toBe(200);
      expect(await page.text(), path).toContain('Styrhous portal');
      expect(page.headers.get('X-Frame-Options')).toBe('DENY');
      expect(page.headers.get('Referrer-Policy')).toBe('strict-origin-when-cross-origin');
      expect(page.headers.get('X-Content-Type-Options')).toBe('nosniff');
    }

    for (const path of ['/api/does-not-exist', '/auth/does-not-exist', '/desktop/v2/does-not-exist']) {
      const missing = await browser.fetch(path);
      expect(missing.status, path).toBe(404);
      expect(missing.headers.get('Cache-Control')).toBe('no-store');
      expect(await missing.json()).toEqual({ reasonCode: 'not_found' });
    }
  });

  it('serves portal pages and assets to signed-in browsers', async () => {
    const { browser } = await signedInBrowser(upstream, 'hosting@example.com');
    for (const path of ['/billing', '/_app/immutable/entry/start.js']) {
      const page = await browser.fetch(path);
      expect(page.status, path).toBe(200);
      expect(await page.text()).toContain('Styrhous portal');
    }
  });

  it('rejects API mutations without the portal origin', async () => {
    const response = await exports.default.fetch(`${origin}/api/organizations`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: 'No origin' })
    });
    expect(response.status).toBe(403);
    expect(await response.json()).toEqual({ reasonCode: 'cross_origin_request_rejected' });
  });

  it('reports health', async () => {
    const response = await new Browser().fetch('/health');
    expect(await response.json()).toEqual({ status: 'healthy' });
  });
});
