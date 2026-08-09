import { describe, expect, it } from 'vitest';
import {
  mapDoctorCapabilities,
  mapDoctorDetectResult,
  mapDoctorEnvStatus,
  mapDoctorRemediation,
  mapDoctorReport,
} from '@/lib/api/doctor-map';
import type { DoctorReport } from '@/lib/api/doctor';

const sampleReport: DoctorReport = {
  dataDir: 'C:\\Users\\demo\\.agenthub',
  runtimes: [
    {
      id: 'nodejs',
      status: 'missing',
      version: null,
      path: null,
      minRequired: '18',
      remediation: {
        kind: 'winget',
        command: 'winget install OpenJS.NodeJS.LTS',
        url: 'https://nodejs.org/',
        text: 'Install Node.js LTS then restart AgentHub.',
      },
    },
    {
      id: 'npm',
      status: 'missing',
      version: null,
      path: null,
      minRequired: null,
      remediation: {
        kind: 'hint',
        command: null,
        url: 'https://nodejs.org/',
        text: 'npm usually ships with Node.js.',
      },
    },
    {
      id: 'powershell',
      status: 'ok',
      version: '5.1',
      path: 'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe',
      minRequired: null,
      remediation: null,
    },
    {
      id: 'git',
      status: 'missing',
      version: null,
      path: null,
      minRequired: null,
      remediation: {
        kind: 'winget',
        command: 'winget install --id Git.Git -e --source winget',
        url: 'https://git-scm.com/downloads',
        text: 'Install Git then restart AgentHub.',
      },
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
    {
      agent: 'kimi',
      status: 'not_found',
      version: null,
      binaryPath: null,
      channel: null,
      envReady: true,
      notes: [],
    },
    {
      agent: 'grok',
      status: 'installed',
      version: '0.2.1',
      binaryPath: 'C:\\Users\\demo\\.grok\\bin\\grok.exe',
      channel: 'native',
      envReady: false,
      notes: ['env not ready'],
    },
    {
      agent: 'pi',
      status: 'not_found',
      version: null,
      binaryPath: null,
      channel: null,
      envReady: false,
      notes: [],
    },
  ],
  paths: {
    dataDir: 'C:\\Users\\demo\\.agenthub',
    dbPath: 'C:\\Users\\demo\\.agenthub\\agenthub.db',
    backupsDir: 'C:\\Users\\demo\\.agenthub\\backups',
    logsDir: 'C:\\Users\\demo\\.agenthub\\logs',
  },
  dbOk: true,
  ok: true,
  warnings: ['agent codex not installed'],
  version: '0.1.0',
};

describe('mapDoctorRemediation', () => {
  it('expands winget remediation into command/url/hint rows', () => {
    const items = mapDoctorRemediation({
      kind: 'winget',
      command: 'winget install OpenJS.NodeJS.LTS',
      url: 'https://nodejs.org/',
      text: 'restart after install',
    });
    expect(items).toEqual([
      {
        kind: 'winget',
        value: 'winget install OpenJS.NodeJS.LTS',
        label: '用 winget 安装',
      },
      { kind: 'url', value: 'https://nodejs.org/', label: '打开官方页面' },
      { kind: 'hint', value: 'restart after install' },
    ]);
  });

  it('preserves brew remediation as a package-manager row', () => {
    const items = mapDoctorRemediation({
      kind: 'brew',
      command: 'brew install node',
      url: 'https://nodejs.org/',
      text: 'Restart AgentHub after installation.',
    });
    expect(items[0]).toEqual({
      kind: 'brew',
      value: 'brew install node',
      label: '用 Homebrew 安装',
    });
  });
});

describe('mapDoctorEnvStatus', () => {
  it('maps core EnvStatus to RuntimeDetect', () => {
    const rt = mapDoctorEnvStatus(sampleReport.runtimes[0]);
    expect(rt.id).toBe('nodejs');
    expect(rt.status).toBe('missing');
    expect(rt.minRequired).toBe('18');
    expect(rt.remediations.some((r) => r.kind === 'winget')).toBe(true);
  });

  it('falls back to config remediations when doctor has none', () => {
    const rt = mapDoctorEnvStatus(sampleReport.runtimes[2]);
    expect(rt.status).toBe('ok');
    expect(rt.version).toBe('5.1');
    expect(rt.remediations.length).toBeGreaterThan(0);
  });

  it('maps PowerShell dual-version notes', () => {
    const rt = mapDoctorEnvStatus({
      id: 'powershell',
      status: 'ok',
      version: '7.6.4',
      path: 'C:\\Program Files\\PowerShell\\7\\pwsh.exe',
      minRequired: null,
      remediation: null,
      notes: [
        'Windows PowerShell 5.1: 5.1.26100 @ C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe',
        'PowerShell 7 (pwsh): 7.6.4 @ C:\\Program Files\\PowerShell\\7\\pwsh.exe',
      ],
    });
    expect(rt.notes).toHaveLength(2);
    expect(rt.notes?.[0]).toContain('Windows PowerShell 5.1');
    expect(rt.notes?.[1]).toContain('PowerShell 7');
  });
});

describe('mapDoctorCapabilities', () => {
  it('returns undefined when doctor omits matrix (no silent mock fill)', () => {
    const caps = mapDoctorCapabilities(undefined, 'kimi');
    expect(caps).toBeUndefined();
  });

  it('does not invent a row when doctor matrix exists but agent is missing', () => {
    const caps = mapDoctorCapabilities(
      {
        claude: {
          skills: { level: 'full' },
        },
      },
      'kimi',
    );
    expect(caps).toBeUndefined();
  });

  it('maps doctor-provided cells', () => {
    const caps = mapDoctorCapabilities(
      {
        claude: {
          skills: { level: 'partial', reason: 'doctor override', minVersion: '2.0' },
          accountSwitch: { level: 'full' },
        },
      },
      'claude',
    );
    expect(caps?.skills).toEqual({
      level: 'partial',
      reason: 'doctor override',
      minVersion: '2.0',
    });
    expect(caps?.accountSwitch?.level).toBe('full');
  });

  it('returns undefined without agentId', () => {
    expect(mapDoctorCapabilities({}, undefined)).toBeUndefined();
  });
});

describe('mapDoctorDetectResult', () => {
  it('maps installed agent fields and preserves envReady', () => {
    const runtimes = sampleReport.runtimes.map(mapDoctorEnvStatus);
    const claude = mapDoctorDetectResult(sampleReport.agents[0], runtimes);
    expect(claude).toMatchObject({
      agentId: 'claude',
      installed: true,
      version: '2.1.0',
      channel: 'native',
      binPath: 'C:\\Users\\demo\\.local\\bin\\claude.exe',
      envReady: true,
      authStatus: 'none',
      running: false,
    });
    // no doctor capabilities → undefined (fail-closed mapping)
    expect(claude.capabilities).toBeUndefined();
  });

  it('attaches doctor capabilities when provided', () => {
    const runtimes = sampleReport.runtimes.map(mapDoctorEnvStatus);
    const claude = mapDoctorDetectResult(sampleReport.agents[0], runtimes, {
      claude: {
        skills: { level: 'planned', reason: 'from doctor' },
      },
    });
    expect(claude.capabilities?.skills).toMatchObject({
      level: 'planned',
      reason: 'from doctor',
    });
  });

  it('maps not_found agent as not installed', () => {
    const runtimes = sampleReport.runtimes.map(mapDoctorEnvStatus);
    const codex = mapDoctorDetectResult(sampleReport.agents[1], runtimes);
    expect(codex.installed).toBe(false);
    expect(codex.version).toBeUndefined();
    expect(codex.channel).toBeUndefined();
    // default channel for codex is npm → missing node/npm
    expect(codex.envReady).toBe(false);
    expect(codex.envMissing).toEqual(expect.arrayContaining(['nodejs', 'npm']));
  });
});

describe('mapDoctorReport', () => {
  it('returns parallel runtimes and agents for UI', () => {
    const { runtimes, agents } = mapDoctorReport(sampleReport);
    expect(runtimes).toHaveLength(4);
    expect(runtimes.map((r) => r.id)).toEqual(
      expect.arrayContaining(['nodejs', 'npm', 'powershell', 'git']),
    );
    expect(agents).toHaveLength(sampleReport.agents.length);
    expect(agents.find((a) => a.agentId === 'grok')?.envReady).toBe(false);
    expect(agents.find((a) => a.agentId === 'claude')?.installed).toBe(true);
    expect(agents.find((a) => a.agentId === 'pi')?.installed).toBe(false);
  });

  it('propagates report.capabilities into agent rows', () => {
    const report: DoctorReport = {
      ...sampleReport,
      capabilities: {
        claude: {
          usage: { level: 'full', reason: null, minVersion: null },
        },
      },
    };
    const { agents } = mapDoctorReport(report);
    expect(agents.find((a) => a.agentId === 'claude')?.capabilities?.usage?.level).toBe(
      'full',
    );
  });
});
