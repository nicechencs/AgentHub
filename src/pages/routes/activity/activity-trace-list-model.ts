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
import { localEndpointBrandAgentId } from "@/lib/route-endpoints";
import type { TokenAgentId } from "@/styles/tokens";
import { fmtTokens } from "@/lib/utils";
import { StorageKey } from "@/lib/ui-preferences";

export type ActivityTraceColumnKey =
  | "time"
  | "request"
  | "model"
  | "firstToken"
  | "duration"
  | "tokens"
  | "stages"
  | "route"
  | "details";

export const ACTIVITY_TRACE_WIDTH_SPECS: ColumnWidthSpec<ActivityTraceColumnKey>[] = [
  { key: "time", defaultWidth: 148, minWidth: 112 },
  { key: "request", defaultWidth: 180, minWidth: 148 },
  { key: "model", defaultWidth: 120, minWidth: 96 },
  { key: "firstToken", defaultWidth: 72, minWidth: 64 },
  { key: "duration", defaultWidth: 88, minWidth: 72 },
  { key: "tokens", defaultWidth: 104, minWidth: 88 },
  { key: "stages", defaultWidth: 224, minWidth: 196 },
  { key: "route", defaultWidth: 120, minWidth: 88 },
  { key: "details", defaultWidth: 72, minWidth: 64 },
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
  if (key === "request") return t("routes.activity.colRequest");
  if (key === "model") return t("routes.activity.colModel");
  if (key === "firstToken") return t("routes.activity.colFirstToken");
  if (key === "duration") return t("routes.activity.colDuration");
  if (key === "tokens") return t("routes.activity.colTokens");
  if (key === "stages") return t("routes.activity.colStages");
  if (key === "route") return t("routes.activity.colRoute");
  return t("routes.activity.colDetails");
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
