/**
 * ConnectFlow ①② 引导深链的纯函数契约（不接线页面）。
 *
 * 产品语义：
 * - ConnectFlow「导入已有登录态」→ Connections 自动打开导入确认（不是静默 import）
 * - ConnectFlow「新 API Key」→ Connections 自动打开添加 API Key 对话框
 * - 成功后回到 Dashboard 并重开该 Agent 的 ConnectFlow
 * - 不发起全新 OAuth 授权；导入仍是「读官方 CLI 已完成的登录态」
 * - intent 一次性：处理/消费后从 URL 去掉，刷新不再弹窗
 * - resume 可保留到成功回跳或用户明确放弃
 *
 * URL 约定：
 * - intent 查询键：`intent`（`import-login` | `add-key` | `oauth`）
 * - resume 查询键：`resume`（AgentKey，成功后回 Dashboard 重开 ConnectFlow；可省略）
 * - ① 导入：`/connections?agent=X&intent=import-login&resume=X`
 * - ② 新 Key：`/connections?agent=X&mode=providers&intent=add-key&resume=X`
 * - ③ 官方登录：`/connections?agent=X&intent=oauth`（无 resume，成功后留在连接页）
 * - 回跳：`/?connect=X`
 */
import type { AgentKey } from '@/lib/types';

export type ConnectGuideIntent = 'import-login' | 'add-key' | 'oauth';

export type ConnectGuide = {
  intent: ConnectGuideIntent;
  resumeAgentId: AgentKey | null;
};

const GUIDE_INTENTS = new Set<string>(['import-login', 'add-key', 'oauth']);

export function parseConnectGuideIntent(raw: string | null | undefined): ConnectGuideIntent | null {
  if (raw == null || !GUIDE_INTENTS.has(raw)) return null;
  return raw as ConnectGuideIntent;
}

export function parseResumeAgentId(
  raw: string | null | undefined,
  allowed: readonly AgentKey[],
): AgentKey | null {
  if (raw == null || raw === '') return null;
  return allowed.includes(raw) ? raw : null;
}

/** 解析 Dashboard 回跳查询键 `?connect=`。 */
export function parseConnectResumeParam(
  raw: string | null | undefined,
  allowed: readonly AgentKey[],
): AgentKey | null {
  return parseResumeAgentId(raw, allowed);
}

export function buildConnectionsGuideUrl(input: {
  agentId: AgentKey;
  intent: ConnectGuideIntent;
  resumeAgentId?: AgentKey | null;
}): string {
  const params = new URLSearchParams();
  params.set('agent', input.agentId);
  if (input.intent === 'add-key') {
    params.set('mode', 'providers');
  }
  params.set('intent', input.intent);
  if (input.resumeAgentId) {
    params.set('resume', input.resumeAgentId);
  }
  return `/connections?${params.toString()}`;
}

export function buildResumeConnectUrl(agentId: AgentKey): string {
  const params = new URLSearchParams();
  params.set('connect', agentId);
  return `/?${params.toString()}`;
}

/** 从 URLSearchParams 读 intent + resume；非法 intent 当 null。 */
export function readConnectGuide(
  search: URLSearchParams,
  allowed: readonly AgentKey[],
): ConnectGuide | null {
  const intent = parseConnectGuideIntent(search.get('intent'));
  if (intent == null) return null;
  return {
    intent,
    resumeAgentId: parseResumeAgentId(search.get('resume'), allowed),
  };
}

/** 去掉 intent（保留 agent/mode/resume），返回新 URLSearchParams。 */
export function consumeConnectIntent(search: URLSearchParams): URLSearchParams {
  const next = new URLSearchParams(search);
  next.delete('intent');
  return next;
}

/** 去掉 connect（Dashboard 已打开对话框后）。 */
export function consumeConnectResume(search: URLSearchParams): URLSearchParams {
  const next = new URLSearchParams(search);
  next.delete('connect');
  return next;
}
