import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react';
import { createPortal } from 'react-dom';
import { useI18n } from '@/components/shared/LanguageProvider';
import {
  CHROME_HINT_AUTO_DISMISS_MS,
  CHROME_HINT_SHOW_DELAY_MS,
  dismissChromeHint,
  getChromeHintSnapshot,
  subscribeChromeHint,
} from '@/components/shared/chrome-hint-model';
import { Button } from '@/components/ui/button';

/**
 * 初次打开：短暂提示问号和对话图标。不打开教程。
 * 引导结束后才出现，点「知道了」或超时后不再显示。
 */
export function ChromeHint() {
  const { t } = useI18n();
  const anchorRef = useRef<HTMLSpanElement>(null);
  const pending = useSyncExternalStore(
    subscribeChromeHint,
    getChromeHintSnapshot,
    () => false,
  );
  const [visible, setVisible] = useState(false);
  const [pos, setPos] = useState<{ top: number; right: number } | null>(null);

  useEffect(() => {
    if (!pending) {
      setVisible(false);
      return;
    }
    const show = window.setTimeout(() => setVisible(true), CHROME_HINT_SHOW_DELAY_MS);
    const hide = window.setTimeout(
      () => dismissChromeHint(),
      CHROME_HINT_SHOW_DELAY_MS + CHROME_HINT_AUTO_DISMISS_MS,
    );
    return () => {
      window.clearTimeout(show);
      window.clearTimeout(hide);
    };
  }, [pending]);

  useLayoutEffect(() => {
    if (!pending || !visible) {
      setPos(null);
      return;
    }
    const node = anchorRef.current?.closest('[data-chrome-actions]');
    if (!(node instanceof HTMLElement)) return;

    const update = () => {
      const rect = node.getBoundingClientRect();
      setPos({
        top: rect.bottom + 8,
        right: Math.max(8, window.innerWidth - rect.right),
      });
    };
    update();
    window.addEventListener('resize', update);
    return () => window.removeEventListener('resize', update);
  }, [pending, visible]);

  const card =
    pending && visible && pos && typeof document !== 'undefined'
      ? createPortal(
          <div
            className="fixed z-50 w-64 rounded-card border border-border bg-panel p-3 shadow-md"
            style={{ top: pos.top, right: pos.right }}
            data-chrome-hint
            role="status"
          >
            <p className="text-body text-primary">{t('chrome.hint.help')}</p>
            <p className="mt-1.5 text-body text-primary">{t('chrome.hint.feedback')}</p>
            <div className="mt-3 flex justify-end">
              <Button size="sm" variant="ghost" onClick={() => dismissChromeHint()}>
                {t('chrome.hint.gotIt')}
              </Button>
            </div>
          </div>,
          document.body,
        )
      : null;

  return (
    <>
      <span ref={anchorRef} className="pointer-events-none absolute inset-0" aria-hidden />
      {card}
    </>
  );
}
