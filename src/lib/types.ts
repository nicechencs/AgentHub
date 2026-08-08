// TS 类型 —— 手写同步自 agenthub-core 的 models(见 docs/architecture.md §2 models/)
// Agent 列表与能力以运行时 Catalog 为准（见 platform agent_catalog）；本文件不再封闭 Agent 集合。

/**
 * Open agent key (kebab-case string). Unknown keys are valid wire values;
 * UI shows unavailable/fallback rather than rejecting at the type layer.
 */
export type AgentKey = string;

/**
 * @deprecated Prefer {@link AgentKey}. Kept as an alias so existing imports keep
 * compiling while the closed union is removed — any catalog key is allowed.
 */
export type AgentId = AgentKey;

/** Known built-in keys (display tokens only; not the product set source of truth). */
export const KNOWN_AGENT_IDS = [
  'claude',
  'codex',
  'kimi',
  'grok',
  'pi',
  'workbuddy',
  'cursor',
] as const;

export type KnownAgentId = (typeof KNOWN_AGENT_IDS)[number];

/** 认证状态:有效 / 临期 / 失效 / 未配置 */
export type AuthStatus = 'valid' | 'expiring' | 'expired' | 'none';

/** 安装渠道 */
export type InstallChannel = 'native' | 'npm';

/** 共享运行时(安装 Agent / Skills 的前置环境,与具体 Agent 解耦) */
export type RuntimeId = 'nodejs' | 'npm' | 'powershell' | 'git';

/** Runtime 健康:ok / 缺失 / 版本过旧 / PATH 异常 */
export type EnvStatus = 'ok' | 'missing' | 'outdated' | 'broken_path';

export type RemediationKind = 'winget' | 'command' | 'url' | 'hint';

export interface EnvRemediation {
  kind: RemediationKind;
  /** winget/command 的可执行文本;url 时为链接;hint 时为说明 */
  value: string;
  label?: string;
}

export interface RuntimeDetect {
  id: RuntimeId;
  status: EnvStatus;
  version?: string;
  path?: string;
  minRequired?: string;
  remediations: EnvRemediation[];
  /** core 诊断明细（如 PowerShell 5.1 / 7 分行） */
  notes?: string[];
}

/** 某安装渠道的前置环境检查结果 */
export interface ChannelEnvCheck {
  channelId: string;
  ready: boolean;
  missing: RuntimeId[];
  outdated: RuntimeId[];
  broken: RuntimeId[];
}

/** Dashboard / 总览：当前生效鉴权来源（账号池或 API 供应商池） */
export type EffectiveConnectionKind = 'account' | 'api' | 'none';

/** Agent CLI update probe (npm registry / channel limits). */
export type AgentUpdateState =
  | 'update_available'
  | 'up_to_date'
  | 'unknown'
  | 'unsupported'
  | 'not_installed'
  /** Frontend-only while request in flight */
  | 'checking';

export interface AgentUpdateInfo {
  agentId: AgentId;
  state: AgentUpdateState;
  currentVersion?: string;
  latestVersion?: string;
  /** npm | npm:next | none | native | … */
  source?: string;
  checkedAt?: string;
  note?: string;
  /** Official Setup / download page when auto-update is unsupported */
  setupUrl?: string;
}

export interface AgentStatus {
  agentId: AgentId;
  installed: boolean;
  version?: string;
  latestVersion?: string;
  channel?: InstallChannel;
  binPath?: string;
  authStatus: AuthStatus;
  authLabel: string; // 如 "已登录" / "API" / "未配置"
  /**
   * 兼容字段：当前生效连接的短展示（账号 label，或 供应商名 · URL）。
   * 新代码优先读 effectiveLabel。
   */
  currentProvider?: string;
  /** 当前生效鉴权类型 */
  effectiveKind?: EffectiveConnectionKind;
  /** 当前生效连接展示文案 */
  effectiveLabel?: string;
  /** 进程是否在运行(影响切换警告) */
  running: boolean;
  /**
   * 默认安装渠道的环境是否就绪(list 时由 core 附带)。
   * UI 切换渠道时以 checkChannelEnv 为准。
   */
  envReady?: boolean;
  /** 默认渠道缺失的 Runtime */
  envMissing?: RuntimeId[];
  /** 后端能力矩阵（doctor 附带；浏览器 mock 有镜像） */
  capabilities?: import('@/lib/capability').AgentCapabilities;
  /** Async update probe (filled by checkAgentUpdates). */
  update?: AgentUpdateInfo;
}

export interface Provider {
  id: string;
  agentId: AgentId;
  name: string;
  /** 预设模板 id */
  preset: string;
  /** 配置文本(JSON 或 TOML),敏感字段已脱敏 */
  configText: string;
  configFormat: 'json' | 'toml';
  isCurrent: boolean;
  /**
   * Codex: `settings_config.auth.OPENAI_API_KEY`（双形态兼容）。
   * 脱敏后多为 `***`；编辑留空表示保留原密钥。
   */
  authApiKey?: string;
  /** 上次测速延迟 ms,未测速为 undefined */
  latencyMs?: number;
  /** core 更新时间（比较当前生效项时使用） */
  updatedAt?: string;
  /**
   * 产品层：API Key 是否「官方端点」模式（meta.official）。
   * true = 官方 URL/模型；false = 自定义中转。未设置时由 URL 推断。
   */
  official?: boolean;
}

export type AccountKind = 'oauth' | 'apikey';

export interface Account {
  id: string;
  agentId: AgentId;
  kind: AccountKind;
  /** 展示标签:邮箱或脱敏 key */
  label: string;
  email?: string;
  /**
   * 身份分组键（同人多次授权可相同；仅展示用，不去重）。
   * 来自 core extra.identityLabel，缺省回退 email / label。
   */
  identityLabel?: string;
  subscription?: string; // 订阅等级,如 "ChatGPT Plus"
  isCurrent: boolean;
  tokenValid: boolean;
  /** core 原始 status（如 active） */
  status?: string;
  /** token 剩余秒数 */
  tokenRemainingSec?: number;
  /** 5h 窗口配额用量百分比 0-100 */
  quota5hPct?: number;
  /** 7d 窗口配额用量百分比 0-100 */
  quota7dPct?: number;
  quotaResetIn?: string; // 如 "2h13m 后重置"
  lastUsedAt?: string; // ISO 时间
  /** core 更新时间（比较当前生效项时使用） */
  updatedAt?: string;
  /** core 创建时间（多授权时展示「授权时间」） */
  createdAt?: string;
  /**
   * 凭据格式（脱敏后仍保留）：api_key / credentials_json / auth_json 等。
   * 来自 credentials.format，便于详情核对。
   */
  credentialFormat?: string;
  /** 来源：settings / credentials file / live / manual … */
  source?: string;
  /** Claude settings 环境变量名（如 ANTHROPIC_AUTH_TOKEN） */
  envKey?: string;
  /** 脱敏后的凭据摘要字段（不含明文密钥） */
  credentialSummary?: string;
}

/** 投影状态（与 core SkillSyncState 对齐） */
export type SkillSyncState =
  | 'linked'
  | 'copied'
  | 'absent'
  | 'foreign'
  | 'conflict'
  | 'unsupported';

/**
 * 映射可行性原因（与 core SkillMapStatus 对齐，snake_case wire format）。
 * - available：可映射 / 已映射
 * - private_source：Agent 私有目录，尚未纳入共享源
 * - agent_unsupported：目标 Agent 不支持技能目录
 * - agent_not_installed：目标 Agent 未安装（UI 可叠加 detect 结果）
 * - target_unavailable：目标技能目录不可用
 * - conflict：目标已有不同内容（仍可映射，需确认覆盖）
 */
export type SkillMapStatus =
  | 'available'
  | 'private_source'
  | 'agent_unsupported'
  | 'agent_not_installed'
  | 'target_unavailable'
  | 'conflict';

export type SkillLinkKind = 'none' | 'symlink' | 'junction' | 'hardlink';

export interface SkillProjection {
  agent: AgentId;
  state: SkillSyncState;
  linkKind: SkillLinkKind;
  targetDir?: string | null;
  resolvedTarget?: string | null;
  /** 后端派生的映射原因；缺省时由 state 推断 */
  mapStatus?: SkillMapStatus;
}

export interface Skill {
  id: string;
  name: string;
  description: string;
  /** 真源绝对路径 */
  sourceDir?: string;
  /** 各 agent 投影行（含 linkKind / targetDir） */
  projections: SkillProjection[];
  /**
   * @deprecated 兼容旧 UI：由 projections 派生的扁平 map
   * mapped(linked|copied) → linked/copied；其余原样
   */
  sync: Record<AgentId, SkillSyncState>;
  /** foreign / conflict 的 agent 列表（覆盖确认用） */
  conflicts: AgentId[];
}

export interface UsageRecord {
  id: string;
  timestamp: string; // ISO
  agentId: AgentId;
  model: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  /** Estimated cost in pricing-table units (USD). No FX conversion. */
  costUsd: number;
  sessionId: string;
}

export interface UsageTrendPoint {
  date: string; // YYYY-MM-DD
  /** 各 agent 的输入+输出 token 数 */
  [agentId: string]: number | string;
}

export interface ParserHealth {
  agentId: AgentId;
  supported: boolean;
  records: number;
  failRatePct?: number;
  skipped?: number;
}

export type BackupKind =
  | 'auto-switch'
  | 'manual'
  | 'pre-uninstall'
  | 'pre-restore'
  | 'pre-skill-uninstall';

export interface BackupMeta {
  id: string;
  agentId: AgentId;
  kind: BackupKind;
  createdAt: string; // ISO
  files: string[];
  sizeBytes: number;
  note?: string;
}

/** 运行日志级别（与 core log_level 一致） */
export type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace';

/** 技能市场源：auto 时 skills.sh 不通会回退 skillhub.cn */
export type SkillMarketSource = 'auto' | 'skills.sh' | 'skillhub.cn';

export interface AppSettings {
  language: 'zh' | 'en';
  theme: 'dark' | 'light' | 'system';
  autoStart: boolean;
  closeToTray: boolean;
  hasMasterPassword: boolean;
  credentialStore: 'keyring' | 'encrypted-file';
  dataDir: string;
  /** 日志目录（只读展示；真源为 path_info.logs_dir） */
  logsDir: string;
  /** 日志级别；改后下次启动进程生效 */
  logLevel: LogLevel;
  /** 按日日志保留天数 1–365 */
  logRetentionDays: number;
  /**
   * 技能市场源。
   * - `auto`：优先 skills.sh，网络/API 失败时自动切 skillhub.cn
   * - `skills.sh` / `skillhub.cn`：固定源
   */
  skillMarketSource: SkillMarketSource;
  autoBackup: boolean;
  usageCollectIntervalMin: number;
  appVersion: string;
}

export interface DashboardAlert {
  id: string;
  level: 'warning' | 'danger' | 'info';
  message: string;
  actionLabel: string;
  actionKind: 'refresh-token' | 'view-diff' | 'backup-now' | 'upgrade';
  agentId?: AgentId;
}

/** 切换确认对话框所需的三要素 */
export interface SwitchPreview {
  backfillSummary: string;
  backupPath: string;
  processWarning?: string;
}

// ---------------------------------------------------------------------------
// Chat
// ---------------------------------------------------------------------------

export type ChatRole = 'user' | 'agent';

export type ChatMessageStatus =
  | 'ok'
  | 'failed'
  | 'timeout'
  | 'skipped'
  | 'running'
  | 'cancelled';

export type OutputStream = 'stdout' | 'stderr';

export interface Conversation {
  id: string;
  title: string;
  agentIds: AgentId[];
  cwd?: string | null;
  allowDangerous: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface ChatMessage {
  id: string;
  conversationId: string;
  turn: number;
  role: ChatRole;
  agentId?: AgentId | null;
  content: string;
  status: ChatMessageStatus;
  exitCode?: number | null;
  durationMs: number;
  error?: string | null;
  createdAt: string;
}

/** Normalized process step (Cursor-like process UI). Sync with core ProcessStep. */
export type ProcessStep =
  | { type: 'status'; phase: string; detail?: string | null }
  | { type: 'thinking'; text: string; done?: boolean }
  | {
      type: 'tool';
      id?: string | null;
      name: string;
      input?: unknown;
      status: string;
      result?: string | null;
    }
  | { type: 'text'; text: string }
  | { type: 'raw'; text: string; note?: string | null }
  | { type: 'error'; message: string };

/** Streaming events from chat_send (externally tagged `type`) */
export type ChatEvent =
  | { type: 'started'; turn: number; agents: AgentId[] }
  | { type: 'agentStarted'; turn: number; agent: AgentId; command: string }
  | { type: 'agentChunk'; turn: number; agent: AgentId; stream: OutputStream; text: string }
  | { type: 'agentProcess'; turn: number; agent: AgentId; step: ProcessStep }
  | { type: 'agentFinished'; turn: number; agent: AgentId; message: ChatMessage }
  | { type: 'finished'; turn: number; ok: boolean }
  | { type: 'error'; message: string };

// ---------------------------------------------------------------------------
// Agent Projects（各 CLI 原生 project / session 记录）
// ---------------------------------------------------------------------------

/** 项目容器（按工作区 / 存储目录聚合） */
export interface AgentProject {
  id: string;
  agentId: AgentId;
  title: string;
  storagePath: string;
  actualPath?: string | null;
  relativePath: string;
  sessionCount: number;
  messageCount?: number | null;
  sizeBytes: number;
  updatedAt: string;
  preview?: string | null;
  /** AgentHub 侧别名（不写原生日志） */
  alias?: string | null;
  /** AgentHub 侧隐藏 */
  hidden?: boolean;
}

/** Projects 页旁路元数据整档 */
export interface ProjectUserMeta {
  hidden?: boolean;
  alias?: string | null;
}

export interface ProjectMetadataFile {
  version: number;
  showHiddenProjects: boolean;
  projects: Record<string, ProjectUserMeta>;
}

/** 原生会话文件（删除 / 摘录仍用此 id） */
export interface AgentSession {
  id: string;
  projectId: string;
  agentId: AgentId;
  title: string;
  cwd?: string | null;
  path: string;
  relativePath: string;
  sizeBytes: number;
  updatedAt: string;
  preview?: string | null;
  messageCount?: number | null;
}

/** 多选总结用的摘录 */
export interface AgentProjectExcerpt {
  id: string;
  agentId: AgentId;
  title: string;
  cwd?: string | null;
  updatedAt: string;
  excerpt: string;
}

/** Chat 页 bootstrap（从 Projects 继续对话 / 总结） */
export interface ChatBootstrap {
  agentIds: AgentId[];
  cwd?: string | null;
  title?: string;
  /** 创建会话后自动填入并发送的提示词 */
  prompt?: string;
}
