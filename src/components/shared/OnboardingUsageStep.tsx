import { Check, Cloud, Route } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Card } from '@/components/ui/card';
import type { MessageKey } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import {
  ONBOARDING_USAGE_IDS,
  type OnboardingUsageId,
  type OnboardingUsageSelection,
} from './onboarding-model';

const USAGE_COPY: Record<
  OnboardingUsageId,
  { icon: typeof Route; titleKey: MessageKey; descKey: MessageKey }
> = {
  routes: {
    icon: Route,
    titleKey: 'chrome.onboarding.usageRoutesTitle',
    descKey: 'chrome.onboarding.usageRoutesDesc',
  },
  sub2api: {
    icon: Cloud,
    titleKey: 'chrome.onboarding.usageSub2apiTitle',
    descKey: 'chrome.onboarding.usageSub2apiDesc',
  },
};

export function OnboardingUsageStep({
  selection,
  onToggle,
}: {
  selection: OnboardingUsageSelection;
  onToggle: (id: OnboardingUsageId) => void;
}) {
  const { t } = useI18n();

  return (
    <div className="space-y-3 py-1" data-onboarding-step="usage">
      <p className="text-sm text-secondary">{t('chrome.onboarding.stepUsage')}</p>
      <div className="flex flex-col gap-2" role="group" aria-label={t('chrome.onboarding.stepUsage')}>
        {ONBOARDING_USAGE_IDS.map((id) => {
          const copy = USAGE_COPY[id];
          const Icon = copy.icon;
          const selected = selection[id];
          return (
            <Card
              key={id}
              role="checkbox"
              tabIndex={0}
              aria-checked={selected}
              aria-label={t(copy.titleKey)}
              data-onboarding-choice={id}
              onClick={() => onToggle(id)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  event.preventDefault();
                  onToggle(id);
                }
              }}
              className={cn(
                'flex w-full cursor-pointer items-start gap-3 p-3 text-left transition-colors',
                'hover:border-accent/40 hover:bg-hover/40',
                selected && 'border-accent bg-hover/40',
              )}
            >
              <span
                className={cn(
                  'mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-btn',
                  selected ? 'bg-accent text-white' : 'bg-subtle text-secondary',
                )}
              >
                <Icon className="h-4 w-4" />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-body font-medium">{t(copy.titleKey)}</span>
                <span className="mt-0.5 block text-meta text-muted">{t(copy.descKey)}</span>
              </span>
              {selected ? (
                <Check className="mt-1 h-4 w-4 shrink-0 text-accent" aria-hidden />
              ) : null}
            </Card>
          );
        })}
      </div>
      <p className="text-meta text-muted">{t('chrome.onboarding.usageHint')}</p>
    </div>
  );
}
