import React from 'react';
import ReactDOM from 'react-dom/client';
import { HashRouter } from 'react-router-dom';
import App from './App';
import { BootSplash } from '@/components/shared/BootSplash';
import { ToastProvider } from '@/components/ui/toast';
import { TooltipProvider, TOOLTIP } from '@/components/ui/tooltip';
import { LanguageProvider } from '@/components/shared/LanguageProvider';
import { ThemeProvider } from '@/components/shared/ThemeProvider';
import { UsageSyncProvider } from '@/components/shared/UsageSyncProvider';
import {
  AgentCatalogProvider,
  AgentStatusProvider,
  getBackend,
  loadAgentCatalog,
  loadAgentStatuses,
  loadConnectionPool,
} from '@/app/runtime';
import { applyLanguage, loadStoredLanguage } from '@/lib/i18n';
import { applyTheme, loadStoredTheme } from '@/lib/theme';
import { logger } from '@/lib/logger';
// Design tokens first (SSOT: src/styles/tokens.ts), then structural styles
import 'virtual:agenthub-design-tokens.css';
import '@/styles/globals.css';

const log = logger.scope('boot');

/** 最短展示：避免 hydrate 极快时 splash 一闪而过 */
const MIN_SPLASH_MS = 480;
/**
 * 硬上限：只等 catalog 等轻量预热。agent detect / live-auth 再慢也不得挡住进主界面。
 * 重探测在后台继续，Provider / 页面 skeleton 会吃到最终结果。
 */
const MAX_SPLASH_MS = 2_400;

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

/**
 * 启动关键路径：只阻塞 catalog（驱动 AGENTS 列表）。
 * agent 状态（doctor + 账号池 + live-auth）放到后台，避免启动遮罩假死。
 */
function startBackgroundPreload(): Promise<void> {
  // fire-and-forget：失败写日志，不抛到 splash
  void loadAgentStatuses(getBackend()).catch((e) => {
    log.error('agent status load failed', e);
  });
  void loadConnectionPool(getBackend()).catch((e) => {
    log.error('connection pool load failed', e);
  });

  return loadAgentCatalog(getBackend())
    .then(() => undefined)
    .catch((e) => {
      log.error('agent catalog load failed', e);
    });
}

function Root() {
  const [splashPhase, setSplashPhase] = React.useState<'show' | 'exit' | 'gone'>('show');
  const [progress, setProgress] = React.useState(0.06);

  React.useEffect(() => {
    let cancelled = false;
    const started = performance.now();
    let finished = false;

    const finish = () => {
      if (cancelled || finished) return;
      finished = true;
      setProgress(1);
      setSplashPhase((phase) => (phase === 'show' ? 'exit' : phase));
    };

    // 进度条：时间曲线逼近 0.9，完成时拉满。始终在动，避免“卡死”观感。
    const tick = window.setInterval(() => {
      if (cancelled || finished) return;
      const t = (performance.now() - started) / MAX_SPLASH_MS;
      // 1 - e^(-3t) → 快速起步，缓近 0.9
      const eased = 1 - Math.exp(-3 * Math.min(t, 1.5));
      setProgress(Math.min(0.9, 0.06 + eased * 0.84));
    }, 48);

    const maxTimer = window.setTimeout(finish, MAX_SPLASH_MS);

    void (async () => {
      await startBackgroundPreload();
      const elapsed = performance.now() - started;
      if (elapsed < MIN_SPLASH_MS) {
        await delay(MIN_SPLASH_MS - elapsed);
      }
      finish();
    })();

    return () => {
      cancelled = true;
      window.clearTimeout(maxTimer);
      window.clearInterval(tick);
    };
  }, []);

  const onExited = React.useCallback(() => setSplashPhase('gone'), []);

  return (
    <>
      {splashPhase !== 'gone' && (
        <BootSplash
          exiting={splashPhase === 'exit'}
          progress={progress}
          onExited={onExited}
        />
      )}
      <HashRouter>
        <ThemeProvider>
          <LanguageProvider>
            <TooltipProvider delayDuration={TOOLTIP.delayMs} skipDelayDuration={0}>
              <ToastProvider>
                <AgentCatalogProvider>
                  <AgentStatusProvider>
                    <UsageSyncProvider>
                      <App />
                    </UsageSyncProvider>
                  </AgentStatusProvider>
                </AgentCatalogProvider>
              </ToastProvider>
            </TooltipProvider>
          </LanguageProvider>
        </ThemeProvider>
      </HashRouter>
    </>
  );
}

function boot() {
  // 首屏前同步主题,避免闪烁
  applyTheme(loadStoredTheme());
  applyLanguage(loadStoredLanguage());

  // 立刻挂载 React：由 Root 内 splash 覆盖预加载，不再阻塞在 createRoot 之前
  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <Root />
    </React.StrictMode>,
  );
}

void boot();
