import type { ReactNode } from 'react';
import { Copy } from 'lucide-react';
import {
  routeEndpointBrandAgentId,
  routeEndpointHttpParts,
  type RouteEndpointId,
} from '@/lib/route-endpoints';
import { agentCssVar } from '@/styles/tokens';
import { cn } from '@/lib/utils';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';

/** Same `--agent-*` token the Agent logo/dot uses. Change colors in AGENT_COLORS. */
export function routeEndpointTypeColor(endpointId: RouteEndpointId): string {
  return agentCssVar(routeEndpointBrandAgentId(endpointId));
}

/** Endpoint-type copy colored with the surface's Agent. */
export function RouteEndpointTypeText({
  endpointId,
  className,
  children,
}: {
  endpointId: RouteEndpointId;
  className?: string;
  children: ReactNode;
}) {
  return (
    <span className={className} style={{ color: routeEndpointTypeColor(endpointId) }}>
      {children}
    </span>
  );
}

export function RouteEndpointUrl({
  path,
  port,
  host,
  endpointId,
  className,
}: {
  path: string;
  port?: number | null;
  host?: string;
  endpointId?: RouteEndpointId;
  className?: string;
}) {
  const parts = routeEndpointHttpParts({ path, port, host, endpointId });
  return (
    <span className={cn('inline font-mono', className)}>
      <span className="text-secondary">{parts.origin}</span>
      <span style={{ color: routeEndpointTypeColor(parts.endpointId) }}>{parts.path}</span>
    </span>
  );
}

export function CopyableRouteEndpointUrl({
  path,
  port,
  host,
  endpointId,
  className,
}: {
  path: string;
  port?: number | null;
  host?: string;
  endpointId?: RouteEndpointId;
  className?: string;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const parts = routeEndpointHttpParts({ path, port, host, endpointId });
  const copyText = parts.href ?? parts.display;
  const copy = async (event: { stopPropagation: () => void }) => {
    event.stopPropagation();
    try {
      await navigator.clipboard.writeText(copyText);
      toast({ title: t('routes.endpointCopied'), description: copyText });
    } catch {
      toast({ title: t('routes.copyFailed'), variant: 'danger' });
    }
  };
  return (
    <button
      type="button"
      className="inline-flex max-w-full items-center gap-1 rounded-btn px-1 py-0.5 text-left hover:bg-hover"
      onClick={(event) => { void copy(event); }}
      aria-label={t('routes.copyEndpointAria', { endpoint: copyText })}
    >
      <RouteEndpointUrl
        path={path}
        port={port}
        host={host}
        endpointId={endpointId}
        className={className}
      />
      <Copy className="h-3 w-3 shrink-0 text-muted" aria-hidden />
    </button>
  );
}
