import { afterEach, describe, expect, it } from 'vitest';
import { createBackend as createMockBackend } from '@/dev/mocks/create-backend';
import { createBackend as createTauriBackend } from '@/lib/backend/tauri/create-backend';
import {
  DEFAULT_BACKEND_FEATURES,
  resolveBackendFeatures,
} from '@/lib/backend/contracts';
import { getBackend, resetBackend, setBackend } from '@/app/runtime';
import { getBackendFeatures } from './backend-features';

describe('resolveBackendFeatures', () => {
  it('defaults to fail-closed when missing', () => {
    expect(resolveBackendFeatures(undefined)).toEqual(DEFAULT_BACKEND_FEATURES);
    expect(resolveBackendFeatures(null)).toEqual(DEFAULT_BACKEND_FEATURES);
  });

  it('merges partial overrides', () => {
    expect(resolveBackendFeatures({ providerTestLatency: true })).toEqual({
      ...DEFAULT_BACKEND_FEATURES,
      providerTestLatency: true,
    });
  });
});

describe('getBackendFeatures', () => {
  afterEach(() => {
    resetBackend();
  });

  it('reports production-closed features for tauri backend', () => {
    setBackend(createTauriBackend());
    expect(getBackendFeatures()).toEqual(DEFAULT_BACKEND_FEATURES);
    expect(getBackend().features.providerUndoSwitch).toBe(false);
  });

  it('reports mock undo and latency as available', () => {
    setBackend(createMockBackend());
    expect(getBackendFeatures()).toMatchObject({
      providerUndoSwitch: true,
      providerTestLatency: true,
      accountUndoSwitch: true,
      backupExport: false,
    });
  });
});
