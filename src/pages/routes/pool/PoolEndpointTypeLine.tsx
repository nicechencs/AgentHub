import { RouteEndpointTypeText } from '@/components/shared/RouteEndpointUrl';
import { useI18n } from '@/components/shared/LanguageProvider';
import { handleExternalLinkClick } from '@/lib/open-external';
import {
  localEndpointBrandAgentId,
  localEndpointPath,
  localEndpointSurface,
  type LocalEndpointKind,
} from '@/lib/route-endpoints';
import { cn } from '@/lib/utils';
import { localEndpointKindLabel } from '@/pages/routes/shared/route-pool-view-model';

export function PoolEndpointTypeLine({
  kind,
  href,
  className,
}: {
  kind: LocalEndpointKind;
  href?: string;
  className?: string;
}) {
  const { t } = useI18n();
  const path = localEndpointPath(kind);
  const label = localEndpointKindLabel(kind, t);
  const text = (
    <RouteEndpointTypeText
      endpointId={localEndpointSurface(kind)}
      brandAgentId={localEndpointBrandAgentId(kind)}
      className={cn('whitespace-normal break-all font-mono', className)}
    >
      {path}（{label}）
    </RouteEndpointTypeText>
  );
  if (!href) return text;
  return (
    <a
      href={href}
      className="underline-offset-2 hover:underline"
      onClick={(event) => handleExternalLinkClick(href, event)}
    >
      {text}
    </a>
  );
}
