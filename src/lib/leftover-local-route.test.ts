import { describe, expect, it } from 'vitest';
import { isLeftoverLocalRouteProvider } from './leftover-local-route';
import type { Provider } from '@/lib/types';

describe('isLeftoverLocalRouteProvider', () => {
  it('detects internal generated names and bridge slug markers', () => {
    expect(
      isLeftoverLocalRouteProvider(
        {
          id: 'agenthub_claude_bridge_abc',
          name: 'Claude bridge',
          preset: null,
          configText: '',
          configFormat: 'json',
        } satisfies Pick<Provider, 'id' | 'name' | 'preset' | 'configText' | 'configFormat'>,
      ),
    ).toBe(true);
  });

  it('detects 本机路由 marker in config text', () => {
    const leftover = {
      id: 'prov-1',
      name: 'Custom',
      preset: null,
      configText: 'baseUrl = "http://127.0.0.1:8787"\n# 本机路由',
      configFormat: 'toml',
    } satisfies Pick<Provider, 'id' | 'name' | 'preset' | 'configText' | 'configFormat'>;
    expect(isLeftoverLocalRouteProvider(leftover)).toBe(true);
  });
});
