import type { ReactNode } from 'react';
import { Copy } from 'lucide-react';
import {
  routeEndpointBrandAgentId,
  routeEndpointHttpParts,
  type RouteEndpointId,
} from '@/lib/route-endpoints';
import { agentCssVar, type TokenAgentId } from '@/styles/tokens';
import { cn } from '@/lib/utils';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { Tip } from '@/components/ui/tooltip';

/** Same `--agent-*` token the Agent logo/dot uses. Change colors in AGENT_COLORS. */
export function routeEndpointTypeColor(
  endpointId: RouteEndpointId,
  brandAgentId?: TokenAgentId,
): string {
  return agentCssVar(brandAgentId ?? routeEndpointBrandAgentId(endpointId));
}

/** Endpoint-type copy colored with the surface's Agent. */
export function RouteEndpointTypeText({
  endpointId,
  brandAgentId,
  className,
  children,
}: {
  endpointId: RouteEndpointId;
  brandAgentId?: TokenAgentId;
  className?: string;
  children: ReactNode;
}) {
  return (
    <span className={className} style={{ color: routeEndpointTypeColor(endpointId, brandAgentId) }}>
      {children}
    </span>
  );
}

export function RouteEndpointUrl({
  path,
  port,
  host,
  endpointId,
  brandAgentId,
  className,
}: {
  path: string;
  port?: number | null;
  host?: string;
  endpointId?: RouteEndpointId;
  brandAgentId?: TokenAgentId;
  className?: string;
}) {
  const parts = routeEndpointHttpParts({ path, port, host, endpointId });
  return (
    <span className={cn('inline-block min-w-0 max-w-full truncate font-mono', className)}>
      <span className="text-secondary">{parts.origin}</span>
      <span style={{ color: routeEndpointTypeColor(parts.endpointId, brandAgentId) }}>{parts.path}</span>
    </span>
  );
}

export function CopyableRouteEndpointUrl({
  path,
  port,
  host,
  endpointId,
  brandAgentId,
  className,
}: {
  path: string;
  port?: number | null;
  host?: string;
  endpointId?: RouteEndpointId;
  brandAgentId?: TokenAgentId;
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
    <Tip label={copyText} className="min-w-0 max-w-full">
      <button
        type="button"
        className="inline-flex min-w-0 max-w-full items-center gap-1 overflow-hidden rounded-btn px-1 py-0.5 text-left hover:bg-hover"
        onClick={(event) => { void copy(event); }}
        aria-label={t('routes.copyEndpointAria', { endpoint: copyText })}
      >
        <RouteEndpointUrl
          path={path}
          port={port}
          host={host}
          endpointId={endpointId}
          brandAgentId={brandAgentId}
          className={className}
        />
        <Copy className="h-3 w-3 shrink-0 text-muted" aria-hidden />
      </button>
    </Tip>
  );
}

/** Absolute upstream URL: origin muted, path colored by surface/agent brand. */
export function AbsoluteRouteEndpointUrl({
  url,
  endpointId,
  brandAgentId,
  className,
}: {
  url: string;
  endpointId?: RouteEndpointId;
  brandAgentId?: TokenAgentId;
  className?: string;
}) {
  try {
    const parsed = new URL(url);
    const path = `${parsed.pathname}${parsed.search}`;
    const origin = `${parsed.protocol}//${parsed.host}`;
    const parts = routeEndpointHttpParts({ path, endpointId });
    return (
      <span className={cn('inline font-mono break-all', className)}>
        <span className="text-secondary">{origin}</span>
        <span style={{ color: routeEndpointTypeColor(parts.endpointId, brandAgentId) }}>
          {path || '/'}
        </span>
      </span>
    );
  } catch {
    return <span className={cn('font-mono break-all', className)}>{url}</span>;
  }
}

