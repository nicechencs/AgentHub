import { Link } from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { Hint } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { PoolAuthorizationItem } from '@/pages/bridges/route-pool-view-model';
import { poolAuthorizationLinkIconColors } from './pool-authorization-detail';

function gradientId(key: string): string {
  return `pool-login-link-${key.replace(/[^A-Za-z0-9_-]/g, '_')}`;
}

export function PoolLoginMark({ item }: { item: PoolAuthorizationItem }) {
  const { t } = useI18n();
  if (item.kind === 'oauth') {
    return (
      <span data-pool-login-mark="oauth" className="inline-flex shrink-0">
        <AgentLogo agentId={item.agentId} size="sm" />
      </span>
    );
  }

  const colors = poolAuthorizationLinkIconColors(item);
  const label = t('kind.apikey');
  const mixed = colors.length > 1;
  const id = gradientId(item.key);
  const stroke = mixed ? `url(#${id})` : (colors[0] ?? 'var(--text-muted)');

  return (
    <Hint label={label}>
      <span
        data-pool-login-mark="url"
        className="relative inline-flex h-6 w-6 shrink-0 items-center justify-center"
        aria-label={label}
      >
        {mixed ? (
          <svg width="0" height="0" aria-hidden="true" className="absolute overflow-hidden">
            <defs>
              <linearGradient id={id} x1="0%" y1="0%" x2="100%" y2="100%">
                {colors.map((color, index) => (
                  <stop
                    key={color}
                    offset={`${(index / (colors.length - 1)) * 100}%`}
                    stopColor={color}
                  />
                ))}
              </linearGradient>
            </defs>
          </svg>
        ) : null}
        <Link className="h-4 w-4" strokeWidth={1.8} color={stroke} />
      </span>
    </Hint>
  );
}
