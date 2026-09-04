import type { ReactNode } from 'react';
import { CopyableRouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import type { TraceStageDetail } from './trace-stage-types';

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[6.5rem_minmax(0,1fr)] gap-x-3 gap-y-1 text-meta">
      <dt className="text-muted">{label}</dt>
      <dd className="min-w-0 break-all text-secondary">{children}</dd>
    </div>
  );
}

export function TraceStageDetails({ details }: { details: TraceStageDetail[] }) {
  return (
    <dl className="flex flex-col gap-1.5">
      {details.map((detail, index) => {
        if (detail.kind === 'endpoint') {
          return (
            <Field key={`${detail.kind}-${index}`} label={detail.label}>
              <CopyableRouteEndpointUrl
                path={detail.path}
                port={detail.port}
                host={detail.host}
                endpointId={detail.endpointId}
                brandAgentId={detail.brandAgentId}
              />
            </Field>
          );
        }
        if (detail.kind === 'attempt') {
          const parts = [
            detail.member,
            detail.status,
            detail.requestStatus,
            detail.responseStatus,
            detail.url,
            detail.httpStatus,
            detail.authResult,
            detail.duration,
            detail.code,
            detail.message,
          ].filter((part) => part != null && part !== '');
          return <Field key={`${detail.kind}-${index}`} label={detail.label}>{parts.join(' · ')}</Field>;
        }
        return (
          <Field key={`${detail.kind}-${index}`} label={detail.label}>
            {detail.mono ? <span className="font-mono">{detail.value}</span> : detail.value}
          </Field>
        );
      })}
    </dl>
  );
}
