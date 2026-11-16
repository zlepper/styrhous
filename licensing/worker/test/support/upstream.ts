import { vi } from 'vitest';

type Handler = (request: Request, url: URL) => Response | Promise<Response>;

/**
 * Stands in for third-party HTTP services (Stripe, OAuth providers). The Worker under test
 * shares the test isolate, so stubbing the global `fetch` intercepts its outbound calls.
 * Unexpected requests fail loudly instead of reaching the network.
 */
export class FakeUpstream {
  private readonly routes: { method: string; url: string; handler: Handler }[] = [];
  readonly requests: Request[] = [];

  /** Matches `url` against the request's origin and path, ignoring the query string. */
  on(method: string, url: string, handler: Handler): this {
    this.routes.unshift({ method, url, handler });
    return this;
  }

  install(): this {
    vi.stubGlobal('fetch', async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = new Request(input, init);
      this.requests.push(request.clone());
      const url = new URL(request.url);
      const target = `${url.origin}${url.pathname}`;
      const route = this.routes.find(
        (candidate) =>
          candidate.method === request.method &&
          (candidate.url === target || (candidate.url.endsWith('*') && target.startsWith(candidate.url.slice(0, -1))))
      );
      if (!route) throw new Error(`Unexpected outbound request: ${request.method} ${request.url}`);
      return route.handler(request, url);
    });
    return this;
  }
}

export function json(body: unknown, status = 200): Response {
  return Response.json(body, { status });
}
