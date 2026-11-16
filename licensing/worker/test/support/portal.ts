import { exports } from 'cloudflare:workers';
import { testBindings } from '../test-config.ts';

export const origin = testBindings.PUBLIC_ORIGIN;

/** Status, headers, and parsed JSON body (null when empty) of a response. */
export async function readJson<T = Record<string, any>>(response: Response) {
  const text = await response.text();
  return { status: response.status, headers: response.headers, body: (text ? JSON.parse(text) : null) as T };
}

/** A minimal browser: keeps cookies and sends the portal's Origin on mutations. */
export class Browser {
  private readonly cookies = new Map<string, string>();

  cookie(name: string): string | undefined {
    return this.cookies.get(name);
  }

  setCookie(name: string, value: string) {
    this.cookies.set(name, value);
  }

  async fetch(path: string, init: RequestInit = {}): Promise<Response> {
    const headers = new Headers(init.headers);
    const method = init.method ?? 'GET';
    if (method !== 'GET' && !headers.has('Origin')) headers.set('Origin', origin);
    if (this.cookies.size > 0) {
      headers.set('Cookie', [...this.cookies].map(([name, value]) => `${name}=${value}`).join('; '));
    }
    const response = await exports.default.fetch(new Request(new URL(path, origin), { ...init, method, headers, redirect: 'manual' }));
    for (const cookie of response.headers.getSetCookie()) {
      const [pair, ...attributes] = cookie.split(';');
      const [name, value] = pair!.split('=') as [string, string];
      const expired = attributes.some((attribute) => /max-age=0/i.test(attribute.trim()));
      if (expired || value === '') this.cookies.delete(name);
      else this.cookies.set(name, value);
    }
    return response;
  }

  async json<T = Record<string, any>>(path: string, method = 'GET', body?: unknown) {
    const response = await this.fetch(path, {
      method,
      ...(body === undefined ? {} : { headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
    });
    const { status, body: parsed } = await readJson<T>(response);
    return { status, body: parsed };
  }
}
