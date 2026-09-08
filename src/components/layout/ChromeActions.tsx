import { ChromeHint } from '@/components/shared/ChromeHint';
import { FeedbackButton } from '@/components/shared/FeedbackButton';
import { PageHelpButton } from '@/components/shared/PageHelpButton';

/** 顶栏右侧：问号与反馈。系统级通知不走应用内铃铛。 */
export function ChromeActions() {
  return (
    <div className="relative flex shrink-0 items-center gap-0.5" data-chrome-actions>
      <PageHelpButton />
      <FeedbackButton />
      <ChromeHint />
    </div>
  );
}
