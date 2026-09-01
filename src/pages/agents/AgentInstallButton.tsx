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
  onClick,
}: {
  status?: AgentCardTaskStatus;
  busy?: boolean;
  channelId?: string;
  onClick: () => void;
}) {
  const { t } = useI18n();
  const failed = status === 'failed';
  return (
    <Button
      size="sm"
      variant={installRetryButtonVariant(status)}
      onClick={onClick}
      disabled={busy}
      title={
        failed
          ? t('agents.card.retry')
          : channelId
            ? t('agents.card.installWithChannel', { id: channelId })
            : t('agents.card.install')
      }
    >
      <Zap className="h-3.5 w-3.5" />
      {failed ? t('agents.card.retry') : t('agents.card.install')}
    </Button>
  );
}
