/** Shared backend error helpers (no Tauri, no mock data). */

export const FEATURE_UNAVAILABLE = '功能不可用';
export const FEATURE_UNSUPPORTED = '功能暂未支持';

export class BackendUnavailableError extends Error {
  readonly code = 'backend.unavailable';

  constructor(message: string) {
    super(message);
    this.name = 'BackendUnavailableError';
  }
}

export class BackendUnsupportedError extends Error {
  readonly code = 'backend.unsupported';

  constructor(message: string) {
    super(message);
    this.name = 'BackendUnsupportedError';
  }
}

export function unavailableError(feature: string, detail?: string): BackendUnavailableError {
  const base = `${feature}：${FEATURE_UNAVAILABLE}`;
  return new BackendUnavailableError(detail ? `${base}（${detail}）` : base);
}

export function unsupportedError(feature: string, detail?: string): BackendUnsupportedError {
  const base = `${feature}：${FEATURE_UNSUPPORTED}`;
  return new BackendUnsupportedError(detail ? `${base}（${detail}）` : base);
}
