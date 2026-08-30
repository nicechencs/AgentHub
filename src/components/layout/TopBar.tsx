import { usePageChrome } from '@/components/layout/PageChromeContext';
import { PageTitleBlock } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { FeedbackButton } from '@/components/shared/FeedbackButton';
import { NotificationBell } from '@/components/shared/NotificationBell';
import { cn } from '@/lib/utils';

/** 非对话页顶栏：左侧页标题 + 一行说明，右侧反馈与通知。对话页不渲染。 */
export function TopBar() {
  const chrome = usePageChrome();

  return (
    <header
      className={cn(
        'flex shrink-0 items-center gap-4 border-b border-border bg-panel',
        pageRhythm.topChrome,
        pageRhythm.workbenchX,
      )}
    >
      <div className="min-w-0 flex-1">
        {chrome ? (
          <PageTitleBlock
            title={chrome.title}
            badge={chrome.badge}
            description={chrome.description}
            descriptionTip={chrome.descriptionTip}
          />
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-0.5">
        <FeedbackButton />
        <NotificationBell />
      </div>
    </header>
  );
}
