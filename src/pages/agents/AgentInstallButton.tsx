import { Zap } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  installRetryButtonVariant,
  type AgentCardTaskStatus,
} from './agent-card-model';

/** Same 安装 / 重试 control on the agent list and in details. */
export function AgentInstallButton({
  status,
  busy,
  channelId,
  iconOnly = false,
  onClick,
}: {
  status?: AgentCardTaskStatus;
  busy?: boolean;
  channelId?: string;
  iconOnly?: boolean;
  onClick: () => void;
}) {
  const { t } = useI18n();
  const failed = status === 'failed';
  const label = failed ? t('agents.card.retry') : t('agents.card.install');
  const title = failed
    ? t('agents.card.retry')
    : channelId
      ? t('agents.card.installWithChannel', { id: channelId })
      : t('agents.card.install');
  return (
    <Button
      size={iconOnly ? 'icon' : 'sm'}
      variant={installRetryButtonVariant(status)}
      onClick={onClick}
      disabled={busy}
      title={title}
      aria-label={iconOnly ? label : undefined}
    >
      <Zap className="h-3.5 w-3.5" />
      {iconOnly ? null : label}
    </Button>
  );
}
