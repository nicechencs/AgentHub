/**
 * Pure view-model for model-field fetch status (node / no jsdom).
 */
import { describe, expect, it } from 'vitest';
import {
  REDACTED_MARKER,
  remoteModelsStatusView,
  resolveUpstreamBaseUrl,
  shouldFetchRemoteModels,
} from '@/lib/provider-detect';
import { canSaveProviderForm } from './providerSaveFlow';

describe('remoteModelsStatusView', () => {
  it('loading shows the loading key; picker and retry stay off', () => {
    const view = remoteModelsStatusView({ loading: true, error: false, ids: [] });
    expect(view.kind).toBe('loading');
    expect(view.showRetry).toBe(false);
    expect(view.showPicker).toBe(false);
    expect(view.labelKey).toBe('connections.providerDialog.remoteModelsLoading');
  });

  it('failed makes retry the primary action', () => {
    const view = remoteModelsStatusView({ loading: false, error: true, ids: [] });
    expect(view.kind).toBe('failed');
    expect(view.showRetry).toBe(true);
    expect(view.showPicker).toBe(false);
    expect(view.labelKey).toBe('connections.providerDialog.remoteModelsFailed');
  });

  it('empty list is ok (hint only, no picker)', () => {
    const view = remoteModelsStatusView({ loading: false, error: false, ids: [] });
    expect(view.kind).toBe('empty');
    expect(view.showRetry).toBe(false);
    expect(view.showPicker).toBe(false);
    expect(view.labelKey).toBe('connections.providerDialog.remoteModelsEmpty');
  });

  it('ready with ids shows the picker', () => {
    const view = remoteModelsStatusView({
      loading: false,
      error: false,
      ids: ['openrouter/auto', 'anthropic/claude-sonnet-4'],
    });
    expect(view.kind).toBe('ready');
    expect(view.showRetry).toBe(false);
    expect(view.showPicker).toBe(true);
    expect(view.labelKey).toBeNull();
  });

  it('idle when fetch is inactive (official / gate closed)', () => {
    const view = remoteModelsStatusView({
      loading: false,
      error: false,
      ids: [],
      active: false,
    });
    expect(view.kind).toBe('idle');
    expect(view.showRetry).toBe(false);
    expect(view.showPicker).toBe(false);
    expect(view.labelKey).toBeNull();
  });
});

describe('status when URL comes from advanced config', () => {
  const configText = JSON.stringify(
    {
      baseURL: 'https://openrouter.ai/api/v1',
      model: 'stealth/ox-alpha',
    },
    null,
    2,
  );

  it('loading / fail+retry when form baseUrl is empty but resolved URL is present', () => {
    const resolved = resolveUpstreamBaseUrl({
      formBaseUrl: '',
      configText,
      configFormat: 'json',
      agentId: 'codex',
    });
    expect(resolved).toBe('https://openrouter.ai/api/v1');
    const shouldFetch = shouldFetchRemoteModels({
      useOfficial: false,
      baseUrl: resolved,
      apiKey: REDACTED_MARKER,
      hasStoredSecret: true,
    });
    expect(shouldFetch).toBe(true);

    const loading = remoteModelsStatusView({
      loading: true,
      error: false,
      ids: [],
      active: shouldFetch,
    });
    expect(loading.kind).toBe('loading');
    expect(loading.showRetry).toBe(false);
    expect(loading.labelKey).toBe('connections.providerDialog.remoteModelsLoading');

    const failed = remoteModelsStatusView({
      loading: false,
      error: true,
      ids: [],
      active: shouldFetch,
    });
    expect(failed.kind).toBe('failed');
    expect(failed.showRetry).toBe(true);
    expect(failed.labelKey).toBe('connections.providerDialog.remoteModelsFailed');

    expect(
      canSaveProviderForm({
        schemaStatus: 'ready',
        configError: null,
        isEdit: true,
        apiKey: '',
        piNeedsUrl: false,
        baseUrl: '',
        model: '',
      }),
    ).toBe(true);
  });
});

describe('canSaveProviderForm ignores fetch status', () => {
  it('edit mode + schema ready + empty model stays savable while loading or failed', () => {
    const gate = {
      schemaStatus: 'ready' as const,
      configError: null,
      isEdit: true,
      apiKey: '',
      piNeedsUrl: false,
      baseUrl: 'https://openrouter.ai/api/v1',
      model: '',
    };
    expect(canSaveProviderForm(gate)).toBe(true);
    expect(canSaveProviderForm({ ...gate, model: undefined })).toBe(true);
    const loading = remoteModelsStatusView({ loading: true, error: false, ids: [] });
    const failed = remoteModelsStatusView({ loading: false, error: true, ids: [] });
    expect(loading.kind).toBe('loading');
    expect(failed.showRetry).toBe(true);
    expect(canSaveProviderForm(gate)).toBe(true);
  });
});
