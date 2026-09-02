/**
 * Pure view-model for route-trace flow diagrams (monitoring page).
 */
import type { AdapterBridgeRouteTrace, RouteTraceStageStatus } from '@/lib/backend/contracts/adapter';
import {
  LOCAL_ENDPOINT_KINDS,
  localEndpointBrandAgentId,
  localEndpointSurface,
  type LocalEndpointKind,
} from '@/lib/route-endpoints';
import type { TokenAgentId } from '@/styles/tokens';

export type TraceFlowStageId =
  | 'local_endpoint'
  | 'local_auth'
  | 'pool'
  | 'conversion'
  | 'upstream_auth'
  | 'upstream';

export type TraceFlowStageState = 'idle' | 'active' | 'ok' | 'failed' | 'skipped';

export type TraceFlowEndpointNode = {
  kind: LocalEndpointKind;
  path: string;
  labelKey: string;
  state: TraceFlowStageState;
};

export type TraceFlowPoolMember = {
  label: string;
  state: TraceFlowStageState;
  selected: boolean;
  attemptIndex?: number;
};

export type TraceFlowMatrixCell = {
  row: LocalDownstreamRow;
  col: UpstreamChannelCol;
  pathId: string;
  state: TraceFlowStageState;
};

export type LocalDownstreamRow = 'messages' | 'responses' | 'chat';
export type UpstreamChannelCol = 'anthropic' | 'openai_chat' | 'codex_responses' | 'grok';

export type TraceFlowView = {
  activeEndpoint: LocalEndpointKind | null;
  endpoints: TraceFlowEndpointNode[];
  localAuth: { state: TraceFlowStageState; detail?: string | null };
  pool: {
    state: TraceFlowStageState;
    members: TraceFlowPoolMember[];
    selectedLabel: string | null;
  };
  conversion: {
    state: TraceFlowStageState;
    pathId: string | null;
    passthrough: boolean;
    matrix: TraceFlowMatrixCell[];
    activeRow: LocalDownstreamRow | null;
    activeCol: UpstreamChannelCol | null;
  };
  upstreamAuth: { state: TraceFlowStageState; httpStatus?: number | null; code?: string | null };
  upstream: {
    state: TraceFlowStageState;
    url: string | null;
    accountLabel: string | null;
    upstreamModel: string | null;
    httpStatus?: number | null;
  };
  failureStage: TraceFlowStageId | null;
  legacySummary: boolean;
};

const ENDPOINT_LABEL_KEYS: Record<LocalEndpointKind, string> = {
  messages: 'routes.pool.surface.messages',
  responses_codex: 'routes.pool.surface.responsesCodex',
  responses_grok: 'routes.pool.surface.responsesGrok',
  chat_completions: 'routes.pool.surface.chatCompletions',
};

const MATRIX_ROWS: readonly LocalDownstreamRow[] = ['messages', 'responses', 'chat'];
const MATRIX_COLS: readonly UpstreamChannelCol[] = [
  'anthropic',
  'openai_chat',
  'codex_responses',
  'grok',
];

const ROW_TO_PREFIX: Record<LocalDownstreamRow, string> = {
  messages: 'messages',
  responses: 'responses',
  chat: 'chat',
};

const COL_TO_SUFFIX: Record<UpstreamChannelCol, string> = {
  anthropic: 'anthropic',
  openai_chat: 'openai_chat',
  codex_responses: 'codex_responses',
  grok: 'grok',
};

export function conversionPathId(
  row: LocalDownstreamRow,
  col: UpstreamChannelCol,
): string {
  return `${ROW_TO_PREFIX[row]}_to_${COL_TO_SUFFIX[col]}`;
}

export function parseConversionPath(path: string): {
  row: LocalDownstreamRow | null;
  col: UpstreamChannelCol | null;
  passthrough: boolean;
} {
  const trimmed = path.trim();
  if (!trimmed || trimmed === 'passthrough') {
    return { row: null, col: null, passthrough: trimmed === 'passthrough' };
  }
  const match = /^(\w+)_to_(\w+)$/.exec(trimmed);
  if (!match) return { row: null, col: null, passthrough: false };
  const from = match[1];
  const to = match[2];
  const row = from === 'messages'
    ? 'messages'
    : from === 'responses'
      ? 'responses'
      : from === 'chat'
        ? 'chat'
        : null;
  const col = to === 'anthropic'
    ? 'anthropic'
    : to === 'openai_chat'
      ? 'openai_chat'
      : to === 'codex_responses'
        ? 'codex_responses'
        : to === 'grok'
          ? 'grok'
          : null;
  return { row, col, passthrough: false };
}

export function inferLocalEndpointKind(
  trace: Pick<AdapterBridgeRouteTrace, 'path' | 'conversion' | 'upstream'>,
): LocalEndpointKind | null {
  const path = trace.path.trim();
  if (path.startsWith('/v1/messages')) return 'messages';
  if (path.startsWith('/v1/chat/completions')) return 'chat_completions';
  if (path.startsWith('/v1/responses')) {
    const conv = trace.conversion.path;
    if (conv.includes('grok')) return 'responses_grok';
    if (conv.includes('codex')) return 'responses_codex';
    if (trace.upstream.url?.toLowerCase().includes('grok')) return 'responses_grok';
    return 'responses_codex';
  }
  const parsed = parseConversionPath(trace.conversion.path);
  if (parsed.row === 'messages') return 'messages';
  if (parsed.row === 'chat') return 'chat_completions';
  if (parsed.row === 'responses') {
    if (trace.conversion.path.includes('grok')) return 'responses_grok';
    return 'responses_codex';
  }
  return null;
}

function mapStageStatus(status: RouteTraceStageStatus): TraceFlowStageState {
  switch (status) {
    case 'ok':
      return 'ok';
    case 'failed':
      return 'failed';
    case 'skipped':
      return 'skipped';
    case 'pending':
      return 'active';
    default:
      return 'idle';
  }
}

function failureStageId(stage: string | null | undefined): TraceFlowStageId | null {
  switch (stage) {
    case 'local_auth':
      return 'local_auth';
    case 'pool':
      return 'pool';
    case 'conversion':
      return 'conversion';
    case 'upstream_auth':
      return 'upstream_auth';
    case 'upstream':
      return 'upstream';
    default:
      return null;
  }
}

function endpointNodeState(
  kind: LocalEndpointKind,
  active: LocalEndpointKind | null,
  legacy: boolean,
): TraceFlowStageState {
  if (legacy) return 'skipped';
  if (!active) return 'idle';
  return kind === active ? 'active' : 'idle';
}

function buildMatrix(
  pathId: string | null,
  passthrough: boolean,
  conversionState: TraceFlowStageState,
  activeEndpoint: LocalEndpointKind | null,
): TraceFlowMatrixCell[] {
  const parsed = pathId ? parseConversionPath(pathId) : { row: null, col: null, passthrough: false };
  const passthroughRow = passthrough
    ? (activeEndpoint === 'messages'
      ? 'messages'
      : activeEndpoint === 'chat_completions'
        ? 'chat'
        : activeEndpoint?.startsWith('responses')
          ? 'responses'
          : null)
    : null;

  return MATRIX_ROWS.flatMap((row) => MATRIX_COLS.map((col) => {
    const cellPathId = conversionPathId(row, col);
    let state: TraceFlowStageState = 'idle';
    if (conversionState === 'skipped') {
      state = 'skipped';
    } else if (passthrough && passthroughRow === row) {
      state = conversionState === 'failed' ? 'failed' : 'active';
    } else if (parsed.row === row && parsed.col === col) {
      state = conversionState === 'failed'
        ? 'failed'
        : conversionState === 'ok'
          ? 'ok'
          : 'active';
    }
    return { row, col, pathId: cellPathId, state };
  }));
}

export function buildTraceFlowView(
  trace: AdapterBridgeRouteTrace & { legacySummary?: boolean },
): TraceFlowView {
  const legacy = trace.legacySummary === true;
  const activeEndpoint = legacy ? null : inferLocalEndpointKind(trace);
  const localAuthState = legacy ? 'skipped' : mapStageStatus(trace.localAuth.status);
  const poolState = legacy ? 'skipped' : mapStageStatus(trace.pool.status);
  const conversionState = legacy ? 'skipped' : mapStageStatus(trace.conversion.status);
  const upstreamAuthState = legacy ? 'skipped' : mapStageStatus(trace.upstreamAuth.status);
  const upstreamState = legacy ? 'skipped' : mapStageStatus(trace.upstream.status);

  const pathId = trace.conversion.path?.trim() || null;
  const passthrough = pathId === 'passthrough';
  const parsed = pathId ? parseConversionPath(pathId) : { row: null, col: null, passthrough: false };

  const attempts = trace.pool.attempts ?? [];
  const selectedLabel = trace.pool.selectedMember?.label
    ?? trace.upstream.member?.label
    ?? null;
  const poolMembers: TraceFlowPoolMember[] = attempts.length > 0
    ? attempts.map((attempt, index) => ({
      label: attempt.member.label,
      state: mapStageStatus(attempt.status),
      selected: trace.pool.selectedMember?.label === attempt.member.label
        && attempt.status === 'ok',
      attemptIndex: index + 1,
    }))
    : selectedLabel
      ? [{
        label: selectedLabel,
        state: poolState === 'failed' ? 'failed' : poolState === 'ok' ? 'ok' : poolState,
        selected: true,
      }]
      : [];

  return {
    activeEndpoint,
    endpoints: LOCAL_ENDPOINT_KINDS.map((spec) => ({
      kind: spec.kind,
      path: spec.path,
      labelKey: ENDPOINT_LABEL_KEYS[spec.kind],
      state: endpointNodeState(spec.kind, activeEndpoint, legacy),
    })),
    localAuth: {
      state: localAuthState,
      detail: trace.localAuth.code ?? trace.localAuth.message ?? null,
    },
    pool: {
      state: poolState,
      members: poolMembers,
      selectedLabel,
    },
    conversion: {
      state: conversionState,
      pathId,
      passthrough,
      matrix: buildMatrix(pathId, passthrough, conversionState, activeEndpoint),
      activeRow: parsed.row,
      activeCol: parsed.col,
    },
    upstreamAuth: {
      state: upstreamAuthState,
      httpStatus: trace.upstreamAuth.httpStatus,
      code: trace.upstreamAuth.code,
    },
    upstream: {
      state: upstreamState,
      url: trace.upstream.url ?? null,
      accountLabel: trace.upstream.member?.label ?? selectedLabel,
      upstreamModel: trace.upstream.upstreamModel ?? trace.upstream.model ?? null,
      httpStatus: trace.upstream.httpStatus,
    },
    failureStage: failureStageId(trace.failureStage),
    legacySummary: legacy,
  };
}

export function traceFlowEndpointBrandAgentId(kind: LocalEndpointKind): TokenAgentId {
  return localEndpointBrandAgentId(kind);
}

export function traceFlowEndpointSurface(kind: LocalEndpointKind) {
  return localEndpointSurface(kind);
}

export function uniquePoolDisplayLabels(
  pools: readonly { members: readonly { displayLabel?: string }[] }[],
): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const pool of pools) {
    for (const member of pool.members) {
      const label = member.displayLabel?.trim();
      if (!label || seen.has(label)) continue;
      seen.add(label);
      out.push(label);
    }
  }
  return out;
}

export function uniqueTraceUpstreamUrls(
  rows: readonly { upstream?: { url?: string | null } }[],
): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const row of rows) {
    const url = row.upstream?.url?.trim();
    if (!url || seen.has(url)) continue;
    seen.add(url);
    out.push(url);
  }
  return out;
}

export const TRACE_FLOW_MATRIX_ROWS = MATRIX_ROWS;
export const TRACE_FLOW_MATRIX_COLS = MATRIX_COLS;
