import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AdapterCommandError } from '@/lib/backend/contracts/adapter';
import { createTauriAdapterPort, mapAdapterInvokeError } from './adapter';

const invokeMock = vi.fn();
vi.mock('./invoke', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

const profileWire = {
  id: 'profile-1',
  name: 'Kimi → Codex',
  sourceKind: 'provider' as const,
  sourceId: 'source-1',
  targetAgentId: 'codex' as const,
  route: 'local_bridge',
  mode: 'api',
  status: 'active',
  ruleId: 'kimi-membership-to-codex-bridge-v1',
  ruleVersion: '1',
  generatedProviderId: 'generated-1',
  localPort: 32123,
  autoStart: true,
  createdAt: '2026-08-12T00:00:00.000Z',
  updatedAt: '2026-08-12T00:00:00.000Z',
};

const analysisWire = {
  route: 'unsupported',
  support: 'unsupported',
  reason: 'No supported adapter route.',
  actions: [],
  limitations: [],
  evidence: [],
};

const bridgeWire = {
  profileId: 'bridge-1',
  port: 32123,
  running: true,
  state: 'running',
  upstreamStatus: 'unknown',
  startedAtUnixMs: 1_786_492_800_000,
};

beforeEach(() => invokeMock.mockReset());

describe('Tauri adapter route port', () => {
  it('maps an explicit analyze response after forwarding request parameters', async () => {
    invokeMock.mockResolvedValueOnce(analysisWire);
    const port = createTauriAdapterPort();
    const result = await port.analyze({
      sourceKind: 'provider', sourceId: 'source-1', targetAgentId: 'pi',
    });
    expect(invokeMock).toHaveBeenCalledWith('analyze_adapter', {
      sourceKind: 'provider', sourceId: 'source-1', targetAgentId: 'pi',
    });
    expect(result.route).toBe('unsupported');
  });

  it('maps an explicit plan response after forwarding request parameters', async () => {
    invokeMock.mockResolvedValueOnce({
      analysis: analysisWire,
      targetAgentId: 'pi',
      canApply: false,
      serviceImpact: 'none',
      changes: [],
    });
    const port = createTauriAdapterPort();
    const plan = await port.plan({
      sourceKind: 'provider', sourceId: 'source-1', targetAgentId: 'pi',
    });
    expect(invokeMock).toHaveBeenCalledWith('plan_adapter', {
      sourceKind: 'provider', sourceId: 'source-1', targetAgentId: 'pi',
    });
    expect(plan.canApply).toBe(false);
  });

  it('maps profile results while forwarding optional list filters unchanged', async () => {
    invokeMock.mockResolvedValueOnce([profileWire]);
    const port = createTauriAdapterPort();

    const profiles = await port.listProfiles({
      sourceKind: 'provider', sourceId: 'source-1', targetAgentId: 'codex', mode: 'api',
    });

    expect(invokeMock).toHaveBeenCalledWith('list_adapter_profiles', {
      sourceKind: 'provider', sourceId: 'source-1', targetAgentId: 'codex', mode: 'api',
    });
    expect(profiles).toMatchObject([{ id: 'profile-1', route: 'local_bridge', mode: 'api' }]);
  });

  it('maps the generated Core Provider in an apply response', async () => {
    invokeMock.mockResolvedValueOnce({
      profile: profileWire,
      provider: {
        id: 'generated-1',
        agentId: 'codex',
        name: 'Kimi → Codex',
        settingsConfig: { baseUrl: 'http://127.0.0.1:32123/v1' },
        meta: { preset: 'openai-compatible' },
        isCurrent: true,
        createdAt: '2026-08-12T00:00:00.000Z',
        updatedAt: '2026-08-12T00:00:00.000Z',
      },
    });
    const port = createTauriAdapterPort();

    const result = await port.apply({
      sourceKind: 'provider', sourceId: 'source-1', targetAgentId: 'codex',
    });

    expect(invokeMock).toHaveBeenCalledWith('apply_adapter', {
      sourceKind: 'provider', sourceId: 'source-1', targetAgentId: 'codex',
    });
    expect(result.provider).toMatchObject({
      id: 'generated-1', preset: 'openai-compatible', configFormat: 'json',
    });
  });

  it('forwards a profile id when removing an adapter', async () => {
    invokeMock.mockResolvedValueOnce(undefined);
    const port = createTauriAdapterPort();

    await port.remove('profile-1');

    expect(invokeMock).toHaveBeenCalledWith('remove_adapter', { profileId: 'profile-1' });
  });

  it('maps local bridge lifecycle status and forwards auto-start controls without credentials', async () => {
    invokeMock
      .mockResolvedValueOnce(bridgeWire)
      .mockResolvedValueOnce({ ...bridgeWire, state: 'stopped', running: false })
      .mockResolvedValueOnce(bridgeWire)
      .mockResolvedValueOnce({ ...profileWire, autoStart: false });
    const port = createTauriAdapterPort();

    const started = await port.startBridge('bridge-1');
    const stopped = await port.stopBridge('bridge-1');
    await port.getBridgeStatus('bridge-1');
    const profile = await port.setBridgeAutoStart('bridge-1', false);

    expect(started).toMatchObject({
      state: 'running', endpoint: 'http://127.0.0.1:32123/v1',
    });
    expect(stopped.state).toBe('stopped');
    expect(profile.autoStart).toBe(false);
    expect(invokeMock).toHaveBeenNthCalledWith(1, 'start_adapter_bridge', { profileId: 'bridge-1' });
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'stop_adapter_bridge', { profileId: 'bridge-1' });
    expect(invokeMock).toHaveBeenNthCalledWith(3, 'get_adapter_bridge_status', { profileId: 'bridge-1' });
    expect(invokeMock).toHaveBeenNthCalledWith(4, 'set_adapter_bridge_auto_start', {
      profileId: 'bridge-1', autoStart: false,
    });
    expect(JSON.stringify(invokeMock.mock.calls)).not.toContain('token');
  });

  it('maps default route pool overviews without a hub token', async () => {
    invokeMock.mockResolvedValueOnce({
      enabled: true,
      pools: [{
        id: 'pool-1',
        targetAgentId: 'codex',
        surface: 'responses',
        dialect: 'codex',
        v2Enrolled: true,
        gatewayPort: 43121,
        members: [{ sourceKind: 'provider', sourceId: 'kimi-1', enabled: true }],
        listedModels: ['kimi-k2.5'],
      }],
    });
    const port = createTauriAdapterPort();
    const listed = await port.listDefaultRoutePools();
    expect(invokeMock).toHaveBeenCalledWith('list_default_route_pools', {});
    expect(listed.enabled).toBe(true);
    expect(listed.pools[0]).toMatchObject({
      id: 'pool-1',
      surface: 'responses',
      gatewayPort: 43121,
      members: [{ sourceKind: 'provider', sourceId: 'kimi-1', enabled: true }],
    });
    expect(JSON.stringify(listed)).not.toContain('hubToken');
    expect(JSON.stringify(listed)).not.toContain('ahb_');
  });

  it('forwards enroll_native_to_gateway by profile id and drops any token field', async () => {
    invokeMock.mockResolvedValueOnce({
      id: 'pool-1',
      targetAgentId: 'codex',
      surface: 'responses',
      dialect: 'codex',
      v2Enrolled: true,
      gatewayPort: 43121,
      members: [{ sourceKind: 'provider', sourceId: 'kimi-1', enabled: true }],
    });
    const port = createTauriAdapterPort();
    const enrolled = await port.enrollNativeToGateway('native-1');
    expect(invokeMock).toHaveBeenCalledWith('enroll_native_to_gateway', { profileId: 'native-1' });
    expect(enrolled.v2Enrolled).toBe(true);
    expect(JSON.stringify(enrolled)).not.toContain('hubToken');
  });
});

describe('mapAdapterInvokeError', () => {
  it('keeps a structured GuiError payload including retryable and details', () => {
    expect(() => mapAdapterInvokeError({
      code: 'adapter.port_in_use',
      message: '本机路由无法启动或停止',
      details: '127.0.0.1:32123 already bound',
      retryable: true,
    })).toThrow(AdapterCommandError);
    try {
      mapAdapterInvokeError({
        code: 'adapter.port_in_use',
        message: '本机路由无法启动或停止',
        details: '127.0.0.1:32123 already bound',
        retryable: true,
      });
    } catch (error) {
      expect(error).toMatchObject({
        name: 'AdapterCommandError',
        code: 'adapter.port_in_use',
        message: '本机路由无法启动或停止',
        details: '127.0.0.1:32123 already bound',
        retryable: true,
      });
    }
  });

  it('classifies nested payloads and bracketed strings using retryable codes', () => {
    try {
      mapAdapterInvokeError({
        payload: { code: 'adapter.bridge_restore_source', message: 'restore failed' },
      });
    } catch (error) {
      expect(error).toMatchObject({
        code: 'adapter.bridge_restore_source',
        message: 'restore failed',
        retryable: true,
      });
    }

    try {
      mapAdapterInvokeError('listener compensation failed [adapter.bridge_stop]');
    } catch (error) {
      expect(error).toMatchObject({
        code: 'adapter.bridge_stop',
        message: 'listener compensation failed',
        retryable: false,
      });
    }
  });

  it('defaults unstructured rejections to adapter.command and not retryable', () => {
    try {
      mapAdapterInvokeError('plain failure');
    } catch (error) {
      expect(error).toMatchObject({
        code: 'adapter.command',
        message: 'plain failure',
        retryable: false,
      });
    }
    try {
      mapAdapterInvokeError(new Error('IPC broken'));
    } catch (error) {
      expect(error).toMatchObject({
        code: 'adapter.command',
        message: 'IPC broken',
        retryable: false,
      });
    }
  });
});
