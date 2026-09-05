import type { HTMLAttributes } from 'react';
import { CircleUser, KeyRound } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Hint } from '@/components/ui/tooltip';
import { resolveAgentMeta } from '@/config/agents';
import {
  connectionKindLabel,
  type ConnectionKind,
} from '@/lib/connection-kind';
import type { AgentKey } from '@/lib/types';
import { cn } from '@/lib/utils';

/** Map a ticket credential class onto the shared login-kind tokens. */
export function credentialKindFromClass(
  cls: string | null | undefined,
): ConnectionKind | null {
  if (cls === 'oauth') return 'oauth';
  if (cls === 'api_key' || cls === 'apikey') return 'apikey';
  return null;
}

/**
 * Official-login / API Key mark. Color follows the Agent; icon shape carries the kind.
 * Connections and the connection pool must share this control.
 */
export function CredentialKindMark({
  kind,
  agentId,
  className,
  ...props
}: {
  kind: ConnectionKind;
  agentId: AgentKey;
} & HTMLAttributes<HTMLSpanElement>) {
  const { t } = useI18n();
  const color = resolveAgentMeta(agentId).color;
  const label = connectionKindLabel(kind, t);
  const Icon = kind === 'oauth' ? CircleUser : KeyRound;
  return (
    <Hint label={label}>
      <span
        className={cn('inline-flex', className)}
        style={{ color }}
        aria-label={label}
        {...props}
      >
        <Icon className="h-4 w-4" strokeWidth={1.8} />
      </span>
    </Hint>
  );
}
