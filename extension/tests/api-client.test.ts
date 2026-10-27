import { describe, expect, it } from 'vitest';
import { ApiError, createYapSpaceApi, toApiError } from '../src/lib/api/client';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/problem+json' },
  });
}

describe('toApiError', () => {
  it('reads the stable code and retry hint from a problem body', () => {
    const error = toApiError(429, {
      code: 'rate_limited',
      detail: 'Too many requests',
      retry_after_s: 42,
    });
    expect(error).toBeInstanceOf(ApiError);
    expect(error.code).toBe('rate_limited');
    expect(error.retryAfterS).toBe(42);
  });

  it('keeps validation fields', () => {
    const error = toApiError(422, {
      code: 'validation_failed',
      errors: [{ field: 'username', code: 'format', message: 'bad' }],
    });
    expect(error.fields[0]?.field).toBe('username');
  });

  it('still produces an error for an unknown body', () => {
    expect(toApiError(502, 'gateway').code).toBe('http_502');
  });
});

describe('createYapSpaceApi', () => {
  it('adds the bearer token to every request when signed in', async () => {
    const seen: string[] = [];
    const api = createYapSpaceApi(
      'http://api.test',
      () => 'ys_secret',
      async (input, init) => {
        const request = new Request(input, init);
        seen.push(request.headers.get('authorization') ?? '');
        return jsonResponse(200, { id: 'x', username: 'alice', role: 'user', created_at: '2026-10-27T10:00:00.000Z' });
      },
    );
    await api.me();
    expect(seen).toEqual(['Bearer ys_secret']);
  });

  it('sends no authorization header when signed out', async () => {
    const seen: (string | null)[] = [];
    const api = createYapSpaceApi('http://api.test', () => null, async (input, init) => {
      seen.push(new Request(input, init).headers.get('authorization'));
      return jsonResponse(200, { items: [], has_more: false, next_before_seq: null, next_after_seq: null });
    });
    await api.listMessages('d-1');
    expect(seen).toEqual([null]);
  });

  it('turns a refused request into an ApiError with the server code', async () => {
    const api = createYapSpaceApi('http://api.test', () => null, async () =>
      jsonResponse(401, { code: 'invalid_credentials', detail: 'nope' }),
    );
    await expect(api.login('alice', 'wrong password')).rejects.toMatchObject({
      status: 401,
      code: 'invalid_credentials',
    });
  });

  it('reports a network failure as its own error kind', async () => {
    const api = createYapSpaceApi('http://api.test', () => null, async () => {
      throw new TypeError('Failed to fetch');
    });
    await expect(api.me()).rejects.toMatchObject({ status: 0, code: 'network' });
  });
});
