import { describe, expect, it, vi } from 'vitest';
import {
  poolAuthorizationCopyOnSave,
  poolAuthorizationOauthEditable,
  saveOauthPoolLogin,
} from './pool-authorization-edit';

const item = {
  kind: 'oauth' as const,
  sourceKind: 'account' as const,
  sourceId: 'grok-1',
  addedHere: false,
  priority: 2,
};

describe('pool-authorization-edit', () => {
  it('lets official logins be edited and copies Connections-shared ones on save', () => {
    expect(poolAuthorizationOauthEditable({ kind: 'oauth', sourceKind: 'account' })).toBe(true);
    expect(poolAuthorizationOauthEditable({ kind: 'apikey', sourceKind: 'provider' })).toBe(false);
    expect(poolAuthorizationCopyOnSave(item)).toBe(true);
    expect(poolAuthorizationCopyOnSave({ ...item, addedHere: true })).toBe(false);
  });

  it('copies a Connections-shared official login before saving models', async () => {
    const forkConnectionAuthorization = vi.fn(async () => ({
      sourceKind: 'account' as const,
      sourceId: 'grok-copy',
      originalSourceId: 'grok-1',
      copied: true,
    }));
    const setSourceCustomModels = vi.fn(async () => ({}));
    const setRouteAuthorizationPriority = vi.fn(async () => 1);
    await expect(saveOauthPoolLogin(
      { item, models: ['grok-2', ' grok-2 '], priority: '3' },
      { forkConnectionAuthorization, setSourceCustomModels, setRouteAuthorizationPriority },
    )).resolves.toEqual({
      sourceKind: 'account',
      sourceId: 'grok-copy',
      originalSourceId: 'grok-1',
      copied: true,
      models: ['grok-2'],
    });
    expect(forkConnectionAuthorization).toHaveBeenCalledWith('account', 'grok-1');
    expect(setSourceCustomModels).toHaveBeenCalledWith('account', 'grok-copy', ['grok-2']);
    expect(setRouteAuthorizationPriority).toHaveBeenCalledWith('account', 'grok-copy', 3);
  });

  it('does not copy a pool-owned official login', async () => {
    const forkConnectionAuthorization = vi.fn();
    const setSourceCustomModels = vi.fn(async () => ({}));
    const setRouteAuthorizationPriority = vi.fn(async () => 1);
    await expect(saveOauthPoolLogin(
      { item: { ...item, addedHere: true }, models: ['grok-2'], priority: '' },
      { forkConnectionAuthorization, setSourceCustomModels, setRouteAuthorizationPriority },
    )).resolves.toMatchObject({ sourceId: 'grok-1', copied: false });
    expect(forkConnectionAuthorization).not.toHaveBeenCalled();
    expect(setRouteAuthorizationPriority).not.toHaveBeenCalled();
  });
});
