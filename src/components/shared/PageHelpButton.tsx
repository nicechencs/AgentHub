import { useEffect, useState } from 'react';
import { useLocation } from 'react-router-dom';
import { CircleHelp } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { dismissChromeHint } from '@/components/shared/chrome-hint-model';
import { pageHelpIdFromPath } from '@/components/shared/page-help-model';
import { PageHelpTour } from '@/components/shared/PageHelpTour';
import { Hint } from '@/components/ui/tooltip';

/** 顶栏问号：只在点击后开始当前页气泡指导，不主动弹出。 */
export function PageHelpButton() {
  const { t } = useI18n();
  const { pathname, search } = useLocation();
  const [open, setOpen] = useState(false);
  const helpId = pageHelpIdFromPath(pathname, search);
  const label = t('chrome.pageHelp.label');

  useEffect(() => {
    setOpen(false);
  }, [pathname]);

  return (
    <>
      <Hint label={label}>
        <button
          type="button"
          className="relative rounded-btn p-1.5 text-secondary hover:bg-hover hover:text-primary"
          aria-label={label}
          aria-haspopup="dialog"
          aria-expanded={open}
          data-page-help
          data-page-help-id={helpId}
          onClick={() => {
            dismissChromeHint();
            setOpen(true);
          }}
        >
          <CircleHelp className="h-4 w-4" />
        </button>
      </Hint>
      <PageHelpTour open={open} helpId={helpId} onClose={() => setOpen(false)} />
    </>
  );
}
