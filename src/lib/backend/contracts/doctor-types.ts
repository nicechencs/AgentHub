/** Doctor core DTOs (serde camelCase) — pure types, no I/O. */

export type DoctorAgentId =
  | 'claude'
  | 'codex'
  | 'kimi'
  | 'grok'
  | 'pi'
  | 'workbuddy'
  | 'cursor'
  | 'dsh'
  | 'zcode';

export type DoctorRuntimeId = 'nodejs' | 'npm' | 'powershell' | 'git';

export type DoctorEnvStatusKind = 'ok' | 'missing' | 'outdated' | 'broken_path';

export type DoctorDetectStatus = 'installed' | 'not_found';

export interface DoctorRemediation {
  kind: string;
  command?: string | null;
  url?: string | null;
  text?: string | null;
}

export interface DoctorEnvStatus {
  id: DoctorRuntimeId;
  status: DoctorEnvStatusKind;
  version?: string | null;
  path?: string | null;
  minRequired?: string | null;
  remediation?: DoctorRemediation | null;
  notes?: string[] | null;
}

export interface DoctorDetectedCopy {
  path: string;
  kind: string;
  version?: string | null;
  channel?: string | null;
  source?: string | null;
  updateVia?: string | null;
  uninstallVia?: string | null;
}

export interface DoctorDetectResult {
  agent: DoctorAgentId;
  status: DoctorDetectStatus;
  version?: string | null;
  binaryPath?: string | null;
  channel?: string | null;
  envReady: boolean;
  notes: string[];
  extraCopies?: DoctorDetectedCopy[];
}

export interface DoctorCapabilityState {
  level: 'full' | 'partial' | 'unsupported' | 'planned';
  reason?: string | null;
  minVersion?: string | null;
}

export interface DoctorPathInfo {
  dataDir: string;
  dbPath: string;
  backupsDir: string;
  logsDir: string;
}

/** Usage parser health row (same shape as Dashboard ParserHealth). */
export interface DoctorParserHealth {
  agentId: DoctorAgentId;
  supported: boolean;
  records: number;
  failRatePct?: number | null;
  skipped?: number | null;
}

export interface DoctorLockInspection {
  agent: string;
  path: string;
  /** `held` | `stale` | `malformed` */
  status: string;
  pid?: number | null;
  createdUnixMs?: number | null;
  note?: string | null;
}

export interface DoctorReport {
  dataDir: string;
  runtimes: DoctorEnvStatus[];
  agents: DoctorDetectResult[];
  capabilities?: Record<string, Record<string, DoctorCapabilityState>>;
  /** Present when core doctor attaches usage parser health */
  usageHealth?: DoctorParserHealth[];
  paths: DoctorPathInfo;
  dbOk: boolean;
  ok: boolean;
  warnings: string[];
  version: string;
  /** Live-write lock files under `{dataDir}/locks` */
  locks?: DoctorLockInspection[];
}
