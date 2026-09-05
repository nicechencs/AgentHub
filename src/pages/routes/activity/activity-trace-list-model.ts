import type { TranslateFn } from "@/lib/i18n";
import type {
  AdapterBridgeRouteTrace,
  RouteTraceStageStatus,
} from "@/lib/backend/contracts/adapter";
import type { ColumnWidthSpec } from "@/components/ui/table";
import {
  inferLocalEndpointKind,
  parseConversionPath,
  type UpstreamChannelCol,
} from "@/components/shared/route-trace-visual-model";
import { localEndpointBrandAgentId, ROUTE_ENDPOINT_HOST } from "@/lib/route-endpoints";
import type { TokenAgentId } from "@/styles/tokens";
import { fmtTokens } from "@/lib/utils";
import { StorageKey } from "@/lib/ui-preferences";

export type ActivityTraceColumnKey =
  | "time"
  | "key"
  | "endpoint"
  | "model"
  | "firstToken"
  | "duration"
  | "tokens"
  | "stages"
  | "route";

export type ActivityTraceKeyToken = {
  token: string;
  name: string;
  poolId?: string;
};

export const ACTIVITY_TRACE_WIDTH_SPECS: ColumnWidthSpec<ActivityTraceColumnKey>[] = [
  { key: "time", defaultWidth: 148, minWidth: 112 },
  { key: "key", defaultWidth: 168, minWidth: 120 },
  { key: "endpoint", defaultWidth: 236, minWidth: 176 },
  { key: "model", defaultWidth: 120, minWidth: 96 },
  { key: "firstToken", defaultWidth: 72, minWidth: 64 },
  { key: "duration", defaultWidth: 88, minWidth: 72 },
  { key: "tokens", defaultWidth: 104, minWidth: 88 },
  { key: "stages", defaultWidth: 224, minWidth: 196 },
  { key: "route", defaultWidth: 120, minWidth: 88 },
];

export const ACTIVITY_TRACE_COLUMN_WIDTHS_STORAGE_KEY =
  StorageKey.routesActivityColumnWidths;

export const ACTIVITY_TRACE_STAGES = [
  "local_auth",
  "pool",
  "conversion",
  "upstream_auth",
  "upstream",
] as const;

export type ActivityTraceStageId = (typeof ACTIVITY_TRACE_STAGES)[number];

const CONVERSION_ROW_LABEL: Record<"messages" | "responses" | "chat", string> = {
  messages: "routes.trace.flow.rowMessages",
  responses: "routes.trace.flow.rowResponses",
  chat: "routes.trace.flow.rowChat",
};

const CONVERSION_COL_LABEL: Record<UpstreamChannelCol, string> = {
  anthropic: "routes.trace.flow.colAnthropic",
  openai_chat: "routes.trace.flow.colOpenAiChat",
  codex_responses: "routes.trace.flow.colCodex",
  grok: "routes.trace.flow.colGrok",
};

const UPSTREAM_COL_BRAND: Record<UpstreamChannelCol, TokenAgentId> = {
  anthropic: "claude",
  openai_chat: "codex",
  codex_responses: "codex",
  grok: "grok",
};

export function activityTraceColumnLabel(
  key: ActivityTraceColumnKey,
  t: TranslateFn,
): string {
  if (key === "time") return t("routes.activity.colTime");
  if (key === "key") return t("routes.activity.colKey");
  if (key === "endpoint") return t("routes.activity.colEndpoint");
  if (key === "model") return t("routes.activity.colModel");
  if (key === "firstToken") return t("routes.activity.colFirstToken");
  if (key === "duration") return t("routes.activity.colDuration");
  if (key === "tokens") return t("routes.activity.colTokens");
  if (key === "stages") return t("routes.activity.colStages");
  return t("routes.activity.colRoute");
}

export function activityTraceStageLabel(stage: ActivityTraceStageId, t: TranslateFn): string {
  switch (stage) {
    case "local_auth":
      return t("routes.trace.stageId.local_auth");
    case "pool":
      return t("routes.trace.stageId.pool");
    case "conversion":
      return t("routes.trace.stageId.conversion");
    case "upstream_auth":
      return t("routes.trace.stageId.upstream_auth");
    default:
      return t("routes.trace.stageId.upstream");
  }
}

export function activityTraceStageStatusLabel(
  status: RouteTraceStageStatus,
  t: TranslateFn,
): string {
  if (status === "ok") return t("routes.inbound.ok");
  if (status === "failed") return t("routes.inbound.fail");
  if (status === "skipped") return t("routes.trace.notReached");
  if (status === "interrupted") return t("routes.trace.detail.interrupted");
  return t("routes.trace.flow.authPending");
}

export function formatTraceSeconds(
  ms: number | null | undefined,
  t: TranslateFn,
): string {
  if (ms == null) return "";
  const seconds = ms / 1000;
  const label = seconds < 10 ? seconds.toFixed(1) : String(Math.round(seconds));
  return t("routes.activity.seconds", { s: label });
}

export function formatTraceTokens(
  inputTokens: number | null | undefined,
  outputTokens: number | null | undefined,
  t: TranslateFn,
): string {
  if (inputTokens == null && outputTokens == null) return "";
  return t("routes.activity.tokensValue", {
    in: fmtTokens(inputTokens ?? 0),
    out: fmtTokens(outputTokens ?? 0),
  });
}

export function activityTraceModelLabel(row: {
  model?: string | null;
  upstream?: { model?: string | null; upstreamModel?: string | null };
}): string {
  return row.model?.trim()
    || row.upstream?.upstreamModel?.trim()
    || row.upstream?.model?.trim()
    || "";
}

function maskActivityTraceKey(token: string): string {
  const trimmed = token.trim();
  if (!trimmed) return "";
  const tail = trimmed.slice(-4);
  return trimmed.startsWith("ahb_") ? `ahb_••••${tail}` : `••••${tail}`;
}

function endpointProtocolPath(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return "";
  if (trimmed.startsWith("/")) {
    const [path] = trimmed.split("?");
    return path || trimmed;
  }
  try {
    const href = trimmed.includes("://") ? trimmed : `https://${trimmed}`;
    const path = new URL(href).pathname.trim();
    return path && path !== "/" ? path : "";
  } catch {
    const slash = trimmed.indexOf("/");
    if (slash < 0) return "";
    const [path] = trimmed.slice(slash).split("?");
    return path || "";
  }
}

export function activityTraceKeyParts(
  row: {
    profileId?: string | null;
    localAuth?: { keyLast4?: string | null; profileId?: string | null };
  },
  tokens: readonly ActivityTraceKeyToken[] = [],
): { abbrev: string; name: string; label: string } {
  const last4 = row.localAuth?.keyLast4?.trim() || "";
  const profileId = row.localAuth?.profileId?.trim() || row.profileId?.trim() || "";
  const matches = last4
    ? tokens.filter((token) => token.token.trim().endsWith(last4))
    : [];
  const match = (profileId
    ? matches.find((token) => token.poolId === profileId)
    : undefined) ?? matches[0];
  const abbrev = match
    ? maskActivityTraceKey(match.token)
    : last4
      ? `••••${last4}`
      : "";
  const name = match?.name.trim() ?? "";
  return {
    abbrev,
    name,
    label: [abbrev, name].filter(Boolean).join(" "),
  };
}

export function activityTraceInboundPath(row: {
  path?: string | null;
}): string {
  return endpointProtocolPath(row.path ?? "");
}

export function activityTraceUpstreamPath(row: {
  upstreamRequest?: { url?: string | null };
  upstream?: { url?: string | null };
}): string {
  const url = row.upstreamRequest?.url?.trim() || row.upstream?.url?.trim() || "";
  return endpointProtocolPath(url);
}

export function activityTraceInboundEndpoint(row: {
  path?: string | null;
  localAuth?: { port?: number | null };
}): string {
  const path = activityTraceInboundPath(row);
  if (!path) return "";
  const port = row.localAuth?.port;
  if (typeof port === "number" && port > 0) {
    return `http://${ROUTE_ENDPOINT_HOST}:${port}${path}`;
  }
  return path;
}

export function activityTraceUpstreamEndpoint(row: {
  upstreamRequest?: { url?: string | null };
  upstream?: { url?: string | null };
}): string {
  return row.upstreamRequest?.url?.trim() || row.upstream?.url?.trim() || "";
}

export function activityTraceHoverDetail(title: string, value: string): string {
  const detail = value.trim();
  if (!detail) return title;
  return `${title} ${detail}`;
}

export function activityTraceLocalBrand(
  row: Pick<AdapterBridgeRouteTrace, "path" | "conversion" | "upstream">,
): TokenAgentId | undefined {
  const kind = inferLocalEndpointKind(row);
  return kind ? localEndpointBrandAgentId(kind) : undefined;
}

export function activityTraceUpstreamBrand(
  row: Pick<AdapterBridgeRouteTrace, "conversion" | "upstream">,
): TokenAgentId | undefined {
  const parsed = parseConversionPath(row.conversion.path ?? "");
  if (parsed.col) return UPSTREAM_COL_BRAND[parsed.col];
  const url = row.upstream.url?.toLowerCase() ?? "";
  if (url.includes("anthropic")) return "claude";
  if (url.includes("grok")) return "grok";
  if (url.includes("openai") || url.includes("chatgpt")) return "codex";
  return undefined;
}

/** Human label for which conversion ran (or passthrough). */
export function activityTraceConversionLabel(
  row: Pick<AdapterBridgeRouteTrace, "conversion">,
  t: TranslateFn,
): string | null {
  const path = (row.conversion.path ?? "").trim();
  if (!path) return null;
  const parsed = parseConversionPath(path);
  if (parsed.passthrough) return t("routes.trace.flow.passthrough");
  if (parsed.row && parsed.col) {
    return t("routes.trace.flow.conversionOption", {
      from: t(CONVERSION_ROW_LABEL[parsed.row] as Parameters<TranslateFn>[0]),
      to: t(CONVERSION_COL_LABEL[parsed.col] as Parameters<TranslateFn>[0]),
    });
  }
  return path;
}
