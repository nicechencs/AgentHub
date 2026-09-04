import type { RouteTraceStageId, RouteTraceStageStatus } from '@/lib/backend/contracts/adapter';
import type { RouteEndpointId } from '@/lib/route-endpoints';
import type { TokenAgentId } from '@/styles/tokens';

export type DetailStageStatus = RouteTraceStageStatus | 'unrecorded';

export type TraceStageDetail =
  | { kind: 'text'; label: string; value: string; mono?: boolean }
  | {
      kind: 'endpoint';
      label: string;
      path: string;
      port?: number | null;
      host: string;
      endpointId: RouteEndpointId;
      brandAgentId?: TokenAgentId;
    }
  | {
      kind: 'attempt';
      label: string;
      member: string;
      status: string;
      requestStatus?: string | null;
      responseStatus?: string | null;
      url?: string | null;
      httpStatus?: number | null;
      authResult?: string | null;
      duration?: string | null;
      code?: string | null;
      message?: string | null;
    };

export type TraceStageViewModel = {
  id: RouteTraceStageId;
  title: string;
  status: DetailStageStatus;
  summary?: string | null;
  details: TraceStageDetail[];
};
