import type { Context } from 'hono';
import type { ContentfulStatusCode } from 'hono/utils/http-status';

/** A request failure that is reported to the client as `{ reasonCode, ...details }`. */
export class ApiError extends Error {
  constructor(
    readonly status: ContentfulStatusCode,
    readonly reasonCode: string,
    readonly details: Readonly<Record<string, unknown>> = {}
  ) {
    super(reasonCode);
    this.name = 'ApiError';
  }
}

export function fail(
  status: ContentfulStatusCode,
  reasonCode: string,
  details?: Readonly<Record<string, unknown>>
): never {
  throw new ApiError(status, reasonCode, details);
}

export function errorResponse(c: Context, error: ApiError): Response {
  return c.json({ reasonCode: error.reasonCode, ...error.details }, error.status);
}
