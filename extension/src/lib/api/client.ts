// Typed client for the YapSpace REST API (docs/architecture/api.md). Types come from schema.d.ts, which is generated from
// docs/api/openapi.json. Do not write API types by hand.

import createClient from 'openapi-fetch';
import type { components, paths } from './schema';

export type Discussion = components['schemas']['DiscussionDto'];
export type Message = components['schemas']['MessageDto'];
export type MessagePage = components['schemas']['MessagePage'];
export type User = components['schemas']['UserDto'];
export type AuthResponse = components['schemas']['AuthResponse'];
export type ResolveResponse = components['schemas']['ResolveResponse'];
export type Identity = components['schemas']['IdentityDto'];

/** Thrown for every failed call. `code` is stable and safe to switch on (api.md §F.1). */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly retryAfterS: number | undefined;
  readonly reason: string | undefined;
  readonly fields: { field: string; code: string; message: string }[];

  constructor(init: {
    status: number;
    code: string;
    message: string;
    retryAfterS?: number;
    reason?: string;
    fields?: { field: string; code: string; message: string }[];
  }) {
    super(init.message);
    this.name = 'ApiError';
    this.status = init.status;
    this.code = init.code;
    this.retryAfterS = init.retryAfterS;
    this.reason = init.reason;
    this.fields = init.fields ?? [];
  }
}

interface ProblemBody {
  code?: unknown;
  detail?: unknown;
  retry_after_s?: unknown;
  reason?: unknown;
  errors?: unknown;
}

function asProblem(error: unknown): ProblemBody {
  return typeof error === 'object' && error !== null ? (error as ProblemBody) : {};
}

/** Builds an `ApiError` from a failed response body. Unknown shapes still produce a usable error. */
export function toApiError(status: number, error: unknown): ApiError {
  const problem = asProblem(error);
  const fields = Array.isArray(problem.errors)
    ? (problem.errors as { field: string; code: string; message: string }[])
    : undefined;
  return new ApiError({
    status,
    code: typeof problem.code === 'string' ? problem.code : `http_${status}`,
    message: typeof problem.detail === 'string' ? problem.detail : 'request failed',
    retryAfterS: typeof problem.retry_after_s === 'number' ? problem.retry_after_s : undefined,
    reason: typeof problem.reason === 'string' ? problem.reason : undefined,
    fields,
  });
}

type Result<T> = { data?: T; error?: unknown; response: Response };

async function unwrap<T>(call: Promise<Result<T>>): Promise<T> {
  let result: Result<T>;
  try {
    result = await call;
  } catch {
    // openapi-fetch throws only when the request never got a response.
    throw new ApiError({ status: 0, code: 'network', message: 'could not reach YapSpace' });
  }
  if (result.error !== undefined || !result.response.ok) {
    throw toApiError(result.response.status, result.error);
  }
  return result.data as T;
}

export interface YapSpaceApi {
  resolve(url: string, canonicalUrl?: string | null): Promise<ResolveResponse>;
  createDiscussion(url: string, canonicalUrl?: string | null, title?: string): Promise<Discussion>;
  listMessages(
    discussionId: string,
    query?: { beforeSeq?: number; afterSeq?: number; limit?: number },
  ): Promise<MessagePage>;
  postMessage(
    discussionId: string,
    body: string,
    clientMsgId: string,
    parentId?: string,
  ): Promise<Message>;
  deleteMessage(messageId: string): Promise<void>;
  register(username: string, password: string, clientLabel?: string): Promise<AuthResponse>;
  login(username: string, password: string, clientLabel?: string): Promise<AuthResponse>;
  logout(): Promise<void>;
  me(): Promise<User>;
}

/** Creates the API client. `getToken` is read on every request, so a sign-in takes effect at once. */
export function createYapSpaceApi(
  baseUrl: string,
  getToken: () => string | null,
  fetchImpl: typeof fetch = (input, init) => fetch(input, init),
): YapSpaceApi {
  const client = createClient<paths>({ baseUrl, fetch: fetchImpl });
  client.use({
    onRequest({ request }) {
      const token = getToken();
      if (token) request.headers.set('authorization', `Bearer ${token}`);
      return request;
    },
  });

  return {
    resolve: (url, canonicalUrl) =>
      unwrap(
        client.POST('/v1/discussions/resolve', {
          body: { url, canonical_url: canonicalUrl ?? null },
        }),
      ),
    createDiscussion: (url, canonicalUrl, title) =>
      unwrap(
        client.POST('/v1/discussions', {
          body: { url, canonical_url: canonicalUrl ?? null, title: title ?? null },
        }),
      ),
    listMessages: (discussionId, query) =>
      unwrap(
        client.GET('/v1/discussions/{id}/messages', {
          params: {
            path: { id: discussionId },
            query: {
              before_seq: query?.beforeSeq,
              after_seq: query?.afterSeq,
              limit: query?.limit,
            },
          },
        }),
      ),
    postMessage: (discussionId, body, clientMsgId, parentId) =>
      unwrap(
        client.POST('/v1/discussions/{id}/messages', {
          params: { path: { id: discussionId } },
          body: { body, client_msg_id: clientMsgId, parent_id: parentId ?? null },
        }),
      ),
    deleteMessage: async (messageId) => {
      await unwrap(client.DELETE('/v1/messages/{id}', { params: { path: { id: messageId } } }));
    },
    register: (username, password, clientLabel) =>
      unwrap(
        client.POST('/v1/auth/register', {
          body: { username, password, client_label: clientLabel ?? null },
        }),
      ),
    login: (username, password, clientLabel) =>
      unwrap(
        client.POST('/v1/auth/login', {
          body: { username, password, client_label: clientLabel ?? null },
        }),
      ),
    logout: async () => {
      await unwrap(client.POST('/v1/auth/logout'));
    },
    me: () => unwrap(client.GET('/v1/me')),
  };
}
