import { PanelLeftClose, PanelLeftOpen } from 'lucide-react';
import { ChromeActions } from '@/components/layout/ChromeActions';
import { usePageChrome } from '@/components/layout/PageChromeContext';
import { PageTitleBlock } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { useSidebar } from '@/components/layout/SidebarContext';
import { AppLogo } from '@/components/shared/AppLogo';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Hint } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

/** Window title bar: logo over the icon rail, page name, then help/feedback. */
export function TopBar() {
  const chrome = usePageChrome();
  const { collapsed, toggle } = useSidebar();
  const { t } = useI18n();
  const toggleLabel = collapsed ? t('nav.expandSidebar') : t('nav.collapseSidebar');

  return (
    <header
      className={cn(
        'flex shrink-0 items-center border-b border-border bg-canvas',
        pageRhythm.topChrome,
      )}
      data-top-bar=""
    >
      <div className={pageRhythm.railSlot}>
        <AppLogo size={20} className="h-5 w-5" />
      </div>
      <div className="flex min-w-0 flex-1 items-center gap-2 px-2">
        <Hint label={toggleLabel} side="bottom">
          <button
            type="button"
            onClick={toggle}
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-btn text-muted transition-colors hover:bg-hover hover:text-primary focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/30"
            aria-label={toggleLabel}
          >
            {collapsed ? (
              <PanelLeftOpen size={18} strokeWidth={1.6} absoluteStrokeWidth data-icon="nav" />
            ) : (
              <PanelLeftClose size={18} strokeWidth={1.6} absoluteStrokeWidth data-icon="nav" />
            )}
          </button>
        </Hint>
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
        <ChromeActions />
      </div>
    </header>
  );
}
