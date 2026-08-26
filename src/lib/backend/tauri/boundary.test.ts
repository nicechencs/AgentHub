import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { resetBackend, setBackend } from '@/app/runtime';
import { createBackend as createMockBackend } from '@/dev/mocks/create-backend';
import { createBackend as createTauriBackend } from '@/lib/backend/tauri/create-backend';
import { BackendUnavailableError } from '@/lib/backend/contracts/errors';
import type { Backend } from '@/lib/backend/contracts';

const invokeMock = vi.fn();
let tauriRuntime = false;

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  isTauri: () => tauriRuntime,
  Channel: class {
    onmessage: ((ev: unknown) => void) | null = null;
  },
}));

describe('Tauri adapter fail-closed (non-Tauri runtime)', () => {
  beforeEach(() => {
    tauriRuntime = false;
    invokeMock.mockReset();
    setBackend(createTauriBackend());
  });

  afterEach(() => {
    resetBackend();
  });

  it('does not fall back to mock when invoke is called outside Tauri', async () => {
    const backend = createTauriBackend();
    await expect(backend.account.listAccounts()).rejects.toBeInstanceOf(BackendUnavailableError);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('catalog port is fail-closed outside Tauri (no mock fallback)', async () => {
    const backend = createTauriBackend();
    await expect(backend.catalog.listAgentCatalog()).rejects.toBeInstanceOf(
      BackendUnavailableError,
    );
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('surface unavailable via doctor load', async () => {
    const backend = createTauriBackend();
    await expect(backend.doctor.loadDoctorMapped()).rejects.toBeInstanceOf(
      BackendUnavailableError,
    );
  });
});

describe('production Account OAuth', () => {
  beforeEach(() => {
    tauriRuntime = true;
    invokeMock.mockReset();
    setBackend(createTauriBackend());
  });

  afterEach(() => {
    resetBackend();
  });

  it('exposes oauthSupported / startOAuth methods on AccountPort', async () => {
    const backend = createTauriBackend();
    expect(typeof backend.account.oauthSupported).toBe('function');
    expect(typeof backend.account.startOAuth).toBe('function');
    expect(typeof backend.account.waitOAuth).toBe('function');
    expect(typeof backend.account.finishOAuth).toBe('function');
    expect(typeof backend.account.cancelOAuth).toBe('function');
  });
});

describe('production Chat default conversation', () => {
  beforeEach(() => {
    tauriRuntime = true;
    invokeMock.mockReset();
    setBackend(createTauriBackend());
  });

  afterEach(() => {
    resetBackend();
  });

  it('forwards ensureDefaultConversation to the dedicated command', async () => {
    invokeMock.mockResolvedValueOnce({
      id: 'conv-1',
      title: '',
      agentIds: ['claude'],
      cwd: null,
      allowDangerous: false,
      createdAt: 'a',
      updatedAt: 'b',
    });
    const backend = createTauriBackend();
    await expect(backend.chat.ensureDefaultConversation(['claude'], null)).resolves.toMatchObject({
      id: 'conv-1',
    });
    expect(invokeMock).toHaveBeenCalledWith('ensure_default_conversation', {
      agentIds: ['claude'],
      cwd: null,
    });
  });
});

describe('production Usage availability (Tauri port shape)', () => {
  beforeEach(() => {
    tauriRuntime = true;
    setBackend(createTauriBackend());
  });

  afterEach(() => {
    resetBackend();
  });

  it('getAvailability is callable (invoke-backed when in Tauri)', async () => {
    const backend = createTauriBackend();
    expect(typeof backend.usage.getAvailability).toBe('function');
    expect(typeof backend.usage.collectUsage).toBe('function');
    // Outside real Tauri IPC this will throw unavailable from assertTauriRuntime
    // when isTauri is mocked true, invoke is mocked — return a stub.
    invokeMock.mockResolvedValueOnce({ status: 'available' });
    await expect(backend.usage.getAvailability()).resolves.toEqual({ status: 'available' });
  });
});

describe('production runtime install channel forwarding', () => {
  beforeEach(() => {
    tauriRuntime = true;
    invokeMock.mockReset();
    setBackend(createTauriBackend());
  });

  afterEach(() => {
    resetBackend();
  });

  it('omits an unspecified channel so Rust selects the host default', async () => {
    invokeMock.mockResolvedValueOnce({ ok: true, action: 'env_install', logs: [], message: 'ok' });
    const backend = createTauriBackend();
    await backend.install.installRuntime('nodejs');
    expect(invokeMock).toHaveBeenCalledWith('install_runtime', { runtimeId: 'nodejs' });
  });

  it('preserves an explicit Windows winget channel', async () => {
    invokeMock.mockResolvedValueOnce({ ok: true, action: 'env_install', logs: [], message: 'ok' });
    const backend = createTauriBackend();
    await backend.install.installRuntime('git', 'winget');
    expect(invokeMock).toHaveBeenCalledWith('install_runtime', {
      runtimeId: 'git',
      channel: 'winget',
    });
  });
});

describe('production Dashboard alerts', () => {
  beforeEach(() => {
    tauriRuntime = true;
    setBackend(createTauriBackend());
  });

  afterEach(() => {
    resetBackend();
  });

  it('propagates live-agent failures instead of returning demo alerts', async () => {
    const doctorError = new Error('doctor unavailable');
    invokeMock.mockImplementation((command: string) => {
      if (command === 'get_doctor_report') return Promise.reject(doctorError);
      if (command === 'list_hidden_agents') return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    const backend = createTauriBackend();
    await expect(backend.dashboard.listAlerts()).rejects.toBe(doctorError);
  });
});

describe('Mock backend factory isolation', () => {
  afterEach(() => {
    resetBackend();
  });

  it('factory create yields clean chat state without production resetForTests', async () => {
    const a = createMockBackend();
    expect('resetForTests' in a.chat).toBe(false);
    expect('resetForTests' in a.project).toBe(false);

    const conv = await a.chat.createConversation(['claude']);
    expect((await a.chat.listConversations()).some((c) => c.id === conv.id)).toBe(true);

    // 再创建一次 factory → 状态被重置
    const b = createMockBackend();
    expect(await b.chat.listConversations()).toEqual([]);
  });

  it('usage mock is available with demo data', async () => {
    const backend = createMockBackend();
    await expect(backend.usage.getAvailability()).resolves.toEqual({ status: 'available' });
    const rows = await backend.usage.queryUsage({ days: 7 });
    expect(Array.isArray(rows)).toBe(true);
    expect(rows.length).toBeGreaterThan(0);
  });
});

describe('Tauri & Mock satisfy same production Backend shape', () => {
  afterEach(() => {
    resetBackend();
  });

  it('exposes required ports without fake*/reset hooks', () => {
    const assertShape = (backend: Backend) => {
      expect(backend.account).toBeDefined();
      expect(backend.agent).toBeDefined();
      expect(backend.catalog).toBeDefined();
      expect(backend.env).toBeDefined();
      expect(backend.usage).toBeDefined();
      expect(backend.chat).toBeDefined();
      expect(typeof backend.chat.ensureDefaultConversation).toBe('function');
      expect(backend.project).toBeDefined();
      expect(backend.dashboard).toBeDefined();
      expect(backend.update).toBeDefined();
      expect(typeof backend.usage.getAvailability).toBe('function');
      expect(typeof backend.agent.installAgentDetailed).toBe('function');
      expect(typeof backend.env.installRuntimeDetailed).toBe('function');
      expect(typeof backend.install.onProgress).toBe('function');
      expect(typeof backend.skill.onFsChanged).toBe('function');
      expect(typeof backend.update.checkForUpdate).toBe('function');
      expect(typeof backend.update.downloadAndInstall).toBe('function');
      // removed from production contracts
      expect('fakeInstallScript' in backend.agent).toBe(false);
      expect('fakeEnvInstallScript' in backend.env).toBe(false);
      expect('fakeEnvBatchInstallScript' in backend.env).toBe(false);
      expect('simulateBrokenPath' in backend.env).toBe(false);
      expect('resetRuntimesDemo' in backend.env).toBe(false);
      expect('resetForTests' in backend.chat).toBe(false);
      expect('resetForTests' in backend.project).toBe(false);
    };

    tauriRuntime = true;
    assertShape(createTauriBackend());
    assertShape(createMockBackend());
  });
});
