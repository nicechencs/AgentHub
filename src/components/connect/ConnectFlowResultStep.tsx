import { useI18n } from '@/components/shared/LanguageProvider';
import { Notice } from '@/components/shared/Notice';
import { connectFlowResultMessage, type ConnectFlowState } from './connect-flow-state';

export function ConnectFlowResultStep({ result }: { result: NonNullable<ConnectFlowState['result']> }) {
  const { t } = useI18n();
  const message = connectFlowResultMessage(result, t);
  const tone = result.kind === 'failed' ? 'danger' : result.refreshFailed ? 'warning' : 'success';
  return (
    <Notice tone={tone}>
      <p className="text-sm font-medium text-primary">{message}</p>
      {result.kind === 'applied' && result.isCurrent && !result.refreshFailed ? (
        <p className="mt-1">{t('connect.result.appliedCurrent')}</p>
      ) : null}
      {result.kind === 'failed' ? (
        <p className="mt-1">{t('connect.result.failedHint')}</p>
      ) : null}
    </Notice>
  );
}
