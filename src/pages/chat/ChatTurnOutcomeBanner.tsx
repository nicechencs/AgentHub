import { Notice } from '@/components/shared/Notice';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { localizeChatFailure } from './chat-format';
import { turnOutcomeDetail, type ChatTurnOutcome } from './chat-turn-outcome';

export function ChatTurnOutcomeBanner(props: {
  outcome: ChatTurnOutcome;
  retryDisabled: boolean;
  onRetry: () => void;
  onRestoreDraft: () => void;
}) {
  const { t } = useI18n();
  const title = t(`chat.turnOutcome.${props.outcome.kind}` as never);
  const detail = turnOutcomeDetail(
    props.outcome,
    (text) => localizeChatFailure(text, t),
    t(`chat.turnOutcome.${props.outcome.kind}Hint` as never),
  );

  return (
    <Notice tone="warning" className="mb-2">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0 space-y-1">
          <p className="font-medium text-primary">{title}</p>
          <p className="text-meta text-secondary">{detail}</p>
        </div>
        <span className="flex shrink-0 items-center gap-1">
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={props.onRestoreDraft}
          >
            {t('chat.turnOutcome.restoreDraft')}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={props.retryDisabled}
            onClick={props.onRetry}
          >
            {t('chat.turnOutcome.retry')}
          </Button>
        </span>
      </div>
    </Notice>
  );
}
