import type { ReactNode } from 'react';
import { ChromeHint } from '@/components/shared/ChromeHint';
import { FeedbackButton } from '@/components/shared/FeedbackButton';
import { PageHelpButton } from '@/components/shared/PageHelpButton';

/** 顶栏右侧：问号、反馈，以及可选的通知。对话页不带通知。 */
export function ChromeActions({ extra }: { extra?: ReactNode }) {
  return (
    <div className="relative flex shrink-0 items-center gap-0.5" data-chrome-actions>
      <PageHelpButton />
      <FeedbackButton />
      {extra}
      <ChromeHint />
    </div>
  );
}
