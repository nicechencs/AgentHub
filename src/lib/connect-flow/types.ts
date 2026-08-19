/**
 * ConnectFlow 共享契约（docs/hub-redesign-plan.md §6）。
 *
 * 并行实施规则：C1（lib/connect-flow 实现）、C2（components/connect UI）、
 * C3（dashboard）、C4（connections）只允许依赖本文件与既有 lib/api 门面，
 * 不得互相 import 实现文件。本文件的类型名、函数名与返回类型不得擅改；
 * 如实现中发现契约缺陷，停下在交付说明中上报。
 */
import type { Account, AgentId, AgentStatus, Provider, SwitchPreview } from '@/lib/types';
import type {
  AdapterApplyPlan,
  AdapterApplyRequest,
  AdapterApplyResult,
  AdapterProfile,
  AdapterRouteRequest,
} from '@/lib/api/adapter';

/** 全流程凭据身份：以 (kind, id) 为准，防 account/provider id 碰撞。 */
export interface ConnectSourceRef {
  kind: 'account' | 'provider';
  id: string;
}

/** 与 Connections 页行 key 同构：`account:<id>` / `provider:<id>`。 */
export function connectSourceKey(ref: ConnectSourceRef): string {
  return `${ref.kind}:${ref.id}`;
}

/** 对话框进入模式（判别联合）。 */
export type ConnectFlowEntry =
  | { mode: 'for-agent'; targetAgentId: AgentId }
  | { mode: 'for-source'; source: ConnectSourceRef };

/**
 * 来源条目状态：
 * - current：目标 Agent 当前生效，禁用重复提交
 * - switchable：目标 Agent 自有、可走既有原生切换链
 * - blocked_native：原生切换被 capability 门禁禁止（复用 Connections 既有判定与原因文本）
 * - plannable：跨服务候选，可行性由 PlanEligibility 承载
 */
export type SourceOptionState =
  | { kind: 'current' }
  | { kind: 'switchable' }
  | { kind: 'blocked_native'; reason: string }
  | { kind: 'plannable' };

export interface SourceOption {
  ref: ConnectSourceRef;
  /** native = 目标 Agent 自有凭据；cross = 其他服务凭据（跨服务复用组） */
  group: 'native' | 'cross';
  /** 该凭据所属（签发方）Agent */
  agentId: AgentId;
  label: string;
  sublabel?: string;
  state: SourceOptionState;
  /** adapter 生成的 Provider（仅 native 组出现）：标注投影来源 */
  viaAdapter?: { sourceLabel: string };
  account?: Account;
  provider?: Provider;
}

/** 构建来源选项的输入（数据由调用方加载）。 */
export interface SourceOptionsInput {
  targetAgentId: AgentId;
  accounts: readonly Account[];
  providers: readonly Provider[];
  profiles: readonly AdapterProfile[];
  /** 实时 doctor statuses；有 target 且带 capabilities 时优先于 catalog。 */
  agentStatuses?: readonly AgentStatus[];
}

/**
 * 跨服务候选可行性。权威是 plan.canApply（AdapterApplyPlan），
 * 禁止以 analysis.support 推断可执行。
 */
export type PlanEligibility =
  | { kind: 'loading' }
  /** 来源 OAuth 未完成：本地预检命中，不发起 fan-out */
  | { kind: 'blocked_oauth'; message: string }
  | {
      kind: 'ready';
      plan: AdapterApplyPlan;
      canApply: boolean;
      /** 路线摘要（取自 plan.analysis），如"直连端点映射"/"本地桥" */
      routeSummary: string;
      /** canApply=false 时的原因原文（不改写、不隐藏） */
      reason?: string;
    }
  | { kind: 'error'; message: string };

/** fan-out 请求单元。 */
export interface PlanFanoutRequest {
  source: ConnectSourceRef;
  targetAgentId: AgentId;
}

/** 缓存/去重键：含 kind 防 id 碰撞。 */
export function planFanoutKey(request: PlanFanoutRequest): string {
  return `${connectSourceKey(request.source)}->${request.targetAgentId}`;
}

export interface PlanFanoutDeps {
  plan: (request: AdapterRouteRequest) => Promise<AdapterApplyPlan>;
  /** 并发上限，默认 3 */
  concurrency?: number;
  /** OAuth 未完成预检（account 来源）；命中项不发起请求，置 blocked_oauth */
  isOauthIncomplete?: (account: Account) => boolean;
}

/**
 * 命令式 fan-out controller（Node 环境可测；React 侧用
 * useSyncExternalStore 订阅）。要求：并发上限、同 key 去重、
 * generation 防竞态（切换选择后旧响应丢弃）、invalidate 清缓存
 * （对话框每次打开时调用，防陈旧 plan 参与 apply）。
 */
export interface PlanFanoutController {
  /** 声明当前需要的请求集合；已缓存项立即就绪，其余入队。 */
  start(requests: readonly PlanFanoutRequest[], options?: { accounts?: readonly Account[] }): void;
  /** 单项重试（清该 key 缓存后重新请求）。 */
  retry(request: PlanFanoutRequest): void;
  /** 丢弃在途请求（不清缓存）。 */
  cancel(): void;
  /** 清空缓存与在途（对话框重开时调用）。 */
  invalidate(): void;
  subscribe(listener: () => void): () => void;
  /** key = planFanoutKey(request) */
  getState(): ReadonlyMap<string, PlanEligibility>;
}

/** 连接流程成功出口（apply 与原生切换共用）。 */
export type ConnectOutcome =
  | { kind: 'applied'; result: AdapterApplyResult }
  | { kind: 'switched'; ref: ConnectSourceRef; agentId: AgentId };

/**
 * 对话框依赖注入集合：C2 只面向此接口编程（测试注入 fake），
 * 默认实现由 C1 的 createDefaultConnectFlowDeps() 组装，集成阶段接线。
 */
export interface ConnectFlowDeps {
  plan(request: AdapterRouteRequest): Promise<AdapterApplyPlan>;
  apply(request: AdapterApplyRequest): Promise<AdapterApplyResult>;
  listProfiles(): Promise<AdapterProfile[]>;
  /** 原生切换（复用 Connections 既有 lib/api 切换链）。 */
  switchNative(option: SourceOption): Promise<void>;
  /**
   * 原生切换预览（不执行 switch）。
   * account 无 preview API，返回 null；provider 走 switchPreview。
   */
  previewNative?(option: SourceOption): Promise<SwitchPreview | null>;
  buildSourceOptions(input: SourceOptionsInput): SourceOption[];
  isOauthIncomplete(account: Account): boolean;
  createPlanFanout(overrides?: Partial<PlanFanoutDeps>): PlanFanoutController;
}

/** 对话框组件 props（页面挂载契约）。entry=null 表示关闭。 */
export interface ConnectFlowDialogProps {
  entry: ConnectFlowEntry | null;
  deps: ConnectFlowDeps;
  onClose: () => void;
  /**
   * 成功出口回调：页面收到后必须重载 agents+profiles+连接池。
   * 页面刷新失败时应 reject/throw，由对话框结果态呈现
   * "已应用/已切换，但列表刷新失败"，不得误报未生效。
   */
  onConnectionChanged: (outcome: ConnectOutcome) => void | Promise<void>;
  /** ①/② 引导跳转（跳转前对话框自行关闭）。 */
  onNavigate: (to: string) => void;
}

/** 用途反查（钱包行"正用于哪些 Agent"）。 */
export interface ConnectionUsageEntry {
  agentId: AgentId;
  /** direct = 自身 isCurrent 生效；adapter = 本机路由生效 */
  via: 'direct' | 'adapter';
}

export interface ConnectionUsage {
  /** incomplete：profile/生成 Provider/来源缺失或数据部分加载失败，显示"未知/不完整"而非"未使用" */
  status: 'known' | 'incomplete';
  agents: ConnectionUsageEntry[];
}

export interface ConnectionUsageInput {
  accounts: readonly Account[];
  providers: readonly Provider[];
  profiles: readonly AdapterProfile[];
  /** 任一数据域加载失败时置 false → 全部行 status='incomplete' */
  poolComplete: boolean;
}

/** key = connectSourceKey(ref)。adapter 生成的 Provider 不在返回 map 中。 */
export type ConnectionUsageMap = ReadonlyMap<string, ConnectionUsage>;
