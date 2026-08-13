/** The only module that talks HTTP: one named function per endpoint. */

import type { Convention } from './types';

const BASE = '/api';

export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

async function errorMessage(response: Response): Promise<string> {
  try {
    const body = await response.json();
    if (typeof body?.error === 'string') return body.error;
    if (Array.isArray(body?.errors)) return body.errors.join('; ');
  } catch {
    // A proxy or a panic can answer with something that isn't JSON at all.
  }
  return response.statusText || `HTTP ${response.status}`;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${BASE}${path}`, {
    headers: init?.body ? { 'Content-Type': 'application/json' } : undefined,
    ...init,
  });

  if (!response.ok) {
    throw new ApiError(response.status, await errorMessage(response));
  }
  return response.json() as Promise<T>;
}

export function listConventions(): Promise<Convention[]> {
  return request<Convention[]>('/conventions');
}