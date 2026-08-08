import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { resetBackend, setBackend } from '@/app/runtime';
import { createBackend as createMockBackend } from '@/dev/mocks/create-backend';
import { createBackend as createTauriBackend } from '@/lib/backend/tauri/create-backend';
import {
  getDoctorReport,
  loadDoctorMapped,
  refreshDoctor,
  tryLoadDoctorMapped,
} from '@/lib/api/doctor';
import type { DoctorReport } from '@/lib/api/doctor';

const invokeMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  isTauri: () => true,
  Channel: class {
    onmessage: ((ev: unknown) => void) | null = null;
  },
}));

const report: DoctorReport = {
  dataDir: '/tmp/agenthub',
  runtimes: [
    {
      id: 'nodejs',
      status: 'ok',
      version: '20.11.1',
      path: '/usr/bin/node',
      minRequired: '18',
      remediation: null,
    },
    {
      id: 'npm',
      status: 'ok',
      version: '10.2.4',
      path: '/usr/bin/npm',
      minRequired: null,
      remediation: null,
    },
    {
      id: 'powershell',
      status: 'ok',
      version: '7.4',
      path: 'pwsh',
      minRequired: null,
      remediation: null,
    },
  ],
  agents: [
    {
      agent: 'claude',
      status: 'installed',
      version: '2.1.0',
      binaryPath: 'C:\\Users\\demo\\.local\\bin\\claude.exe',
      channel: 'native',
      envReady: true,
      notes: [],
    },
    {
      agent: 'codex',
      status: 'not_found',
      version: null,
      binaryPath: null,
      channel: null,
      envReady: false,
      notes: [],
    },
  ],
  paths: {
    dataDir: '/tmp/agenthub',
    dbPath: '/tmp/agenthub/agenthub.db',
    backupsDir: '/tmp/agenthub/backups',
    logsDir: '/tmp/agenthub/logs',
  },
  dbOk: true,
  ok: true,
  warnings: [],
  version: '0.1.0',
};

describe('mock doctor port', () => {
  beforeEach(() => {
    setBackend(createMockBackend());
  });

  afterEach(() => {
    resetBackend();
  });

  it('tryLoadDoctorMapped returns null without invoke', async () => {
    await expect(tryLoadDoctorMapped()).resolves.toBeNull();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('getDoctorReport rejects in mock mode', async () => {
    await expect(getDoctorReport()).rejects.toThrow(/不可用|unavailable|mock/i);
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

describe('tauri doctor port', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    // Fresh backend = isolated per-port doctor cache (no production reset hooks).
    setBackend(createTauriBackend());
  });

  afterEach(() => {
    vi.clearAllMocks();
    resetBackend();
  });

  it('loadDoctorMapped invokes get_doctor_report and maps result', async () => {
    invokeMock.mockResolvedValue(report);

    const mapped = await loadDoctorMapped();
    expect(invokeMock).toHaveBeenCalledWith('get_doctor_report', { force: false });
    expect(mapped.report).toEqual(report);
    expect(mapped.runtimes).toHaveLength(3);
    expect(mapped.agents.find((a) => a.agentId === 'claude')?.installed).toBe(true);
    expect(mapped.agents.find((a) => a.agentId === 'codex')?.installed).toBe(false);
  });

  it('surfaces invoke failure as rejected promise (error state)', async () => {
    invokeMock.mockRejectedValue(new Error('IPC broken'));

    await expect(tryLoadDoctorMapped()).rejects.toThrow('IPC broken');
    await expect(loadDoctorMapped()).rejects.toThrow('IPC broken');
  });

  it('coalesces concurrent loadDoctorMapped into one invoke', async () => {
    let resolveInvoke!: (v: DoctorReport) => void;
    invokeMock.mockReturnValue(
      new Promise<DoctorReport>((resolve) => {
        resolveInvoke = resolve;
      }),
    );

    const p1 = loadDoctorMapped();
    const p2 = loadDoctorMapped();
    expect(invokeMock).toHaveBeenCalledTimes(1);
    resolveInvoke(report);
    const [a, b] = await Promise.all([p1, p2]);
    expect(a.report.version).toBe('0.1.0');
    expect(b.agents).toHaveLength(report.agents.length);
  });

  it('TTL hit avoids second invoke within 30s', async () => {
    invokeMock.mockResolvedValue(report);

    await loadDoctorMapped();
    await loadDoctorMapped();
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it('refreshDoctor bypasses TTL and forces backend redetect', async () => {
    invokeMock.mockResolvedValue(report);

    await loadDoctorMapped();
    expect(invokeMock).toHaveBeenCalledTimes(1);

    await refreshDoctor();
    expect(invokeMock).toHaveBeenCalledTimes(2);
    expect(invokeMock).toHaveBeenLastCalledWith('get_doctor_report', { force: true });
  });
});
