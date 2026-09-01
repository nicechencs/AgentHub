import { RouteEndpointTypeText } from '@/components/shared/RouteEndpointUrl';
import { useI18n } from '@/components/shared/LanguageProvider';
import {
  localEndpointBrandAgentId,
  localEndpointPath,
  localEndpointSurface,
  type LocalEndpointKind,
} from '@/lib/route-endpoints';
import { cn } from '@/lib/utils';
import { localEndpointKindLabel } from '@/pages/bridges/route-pool-view-model';

export function PoolEndpointTypeLine({
  kind,
  className,
}: {
  kind: LocalEndpointKind;
  className?: string;
}) {
  const { t } = useI18n();
  const path = localEndpointPath(kind);
  const label = localEndpointKindLabel(kind, t);
  return (
    <RouteEndpointTypeText
      endpointId={localEndpointSurface(kind)}
      brandAgentId={localEndpointBrandAgentId(kind)}
      className={cn('whitespace-normal break-all font-mono', className)}
    >
      {path}（{label}）
    </RouteEndpointTypeText>
  );
}
