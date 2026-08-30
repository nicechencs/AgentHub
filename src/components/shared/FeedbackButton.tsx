import { MessageSquarePlus } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { Hint } from '@/components/ui/tooltip';
import { GITHUB_NEW_ISSUE_URL } from '@/lib/github';
import { openExternalLink } from '@/lib/open-external';

/** 顶栏反馈：用系统浏览器打开 GitHub 新建问题页。 */
export function FeedbackButton() {
  const { t } = useI18n();
  const { toast } = useToast();
  const label = t('chrome.feedback.label');

  const handleClick = () => {
    void openExternalLink(GITHUB_NEW_ISSUE_URL).catch((e) => {
      toast({
        title: t('chrome.feedback.openFailed'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    });
  };

  return (
    <Hint label={label}>
      <button
        type="button"
        className="relative rounded-btn p-1.5 text-secondary hover:bg-hover hover:text-primary"
        aria-label={label}
        onClick={handleClick}
      >
        <MessageSquarePlus className="h-4 w-4" />
      </button>
    </Hint>
  );
}
