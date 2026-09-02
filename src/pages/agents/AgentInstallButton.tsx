import { RefreshCw, Zap } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  installPrimaryLabelKey,
  installRetryButtonVariant,
  type AgentCardTaskStatus,
} from './agent-card-model';

/** Same 安装 / 重试 / 重新检测 control on the agent list and in details. */
export function AgentInstallButton({
  status,
  busy,
  channelId,
  iconOnly = false,
  linuxUnsupported = false,
  onClick,
}: {
  status?: AgentCardTaskStatus;
  busy?: boolean;
  channelId?: string;
  iconOnly?: boolean;
  /** Win/Mac-only agents on Linux: show 暂不支持 Linux instead of 安装. */
  linuxUnsupported?: boolean;
  onClick: () => void;
}) {
  const { t } = useI18n();
  const labelKey = installPrimaryLabelKey(status);
  const guided = status === 'guided';
  const failed = status === 'failed';
  const label = linuxUnsupported
    ? t('agents.card.linuxUnsupported')
    : t(labelKey);
  const title = linuxUnsupported
    ? t('agents.card.linuxUnsupportedHint')
    : failed || guided
      ? label
      : channelId
        ? t('agents.card.installWithChannel', { id: channelId })
        : t('agents.card.install');
  const Icon = guided ? RefreshCw : Zap;
  return (
    <Button
      size={iconOnly ? 'icon' : 'sm'}
      variant={linuxUnsupported ? 'outline' : installRetryButtonVariant(status)}
      onClick={onClick}
      disabled={busy}
      title={title}
      aria-label={iconOnly ? label : undefined}
    >
      <Icon className="h-3.5 w-3.5" />
      {iconOnly ? null : label}
    </Button>
  );
}
