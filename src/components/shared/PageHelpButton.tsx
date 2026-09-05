import { useEffect, useState } from 'react';
import { useLocation } from 'react-router-dom';
import { CircleHelp } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { dismissChromeHint } from '@/components/shared/chrome-hint-model';
import { pageHelpIdFromPath } from '@/components/shared/page-help-model';
import { isPageHelpOpenKey } from '@/components/shared/page-help-tour';
import { PageHelpTour } from '@/components/shared/PageHelpTour';
import { Button } from '@/components/ui/button';
import { Hint } from '@/components/ui/tooltip';

function openPageHelp(setOpen: (open: boolean) => void) {
  dismissChromeHint();
  setOpen(true);
}

/** 顶栏问号：点击或按 F1 开始当前页气泡指导，不主动弹出。 */
export function PageHelpButton() {
  const { t } = useI18n();
  const { pathname, search } = useLocation();
  const [open, setOpen] = useState(false);
  const helpId = pageHelpIdFromPath(pathname, search);
  const label = t('chrome.pageHelp.label');

  useEffect(() => {
    setOpen(false);
  }, [pathname]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!isPageHelpOpenKey(event)) return;
      event.preventDefault();
      openPageHelp(setOpen);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  return (
    <>
      <Hint
        label={
          <span className="inline-flex items-center gap-1.5">
            {label}
            <kbd className="inline-flex h-5 min-w-5 items-center justify-center rounded-btn border border-border bg-subtle px-1 text-meta leading-none text-muted">
              F1
            </kbd>
          </span>
        }
      >
        <Button
          type="button"
          size="icon"
          variant="ghost"
          className="relative"
          aria-label={label}
          aria-keyshortcuts="F1"
          aria-haspopup="dialog"
          aria-expanded={open}
          data-page-help
          data-page-help-id={helpId}
          onClick={() => openPageHelp(setOpen)}
        >
          <CircleHelp className="h-4 w-4" />
        </Button>
      </Hint>
      <PageHelpTour open={open} helpId={helpId} onClose={() => setOpen(false)} />
    </>
  );
}
