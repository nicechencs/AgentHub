import React from 'react';
import ReactDOM from 'react-dom/client';
import { HashRouter } from 'react-router-dom';
import App from './App';
import { BootSplash } from '@/components/shared/BootSplash';
import { AppErrorBoundary } from '@/components/shared/AppErrorBoundary';
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
  loadTicketWallet,
} from '@/app/runtime';
import { fetchCatalogShared } from '@/lib/hooks/useSkills';
import { fetchAgentProjectsShared, rememberedProjectAgent } from '@/lib/hooks/useProjects';
import { reconcileAccountPool } from '@/lib/api/account';
import { applyLanguage, loadStoredLanguage } from '@/lib/i18n';
import { applyAccent, loadStoredAccent, registerShellIconSync } from '@/lib/accent';
import { applyShellAccentIconBestEffort } from '@/lib/backend/tauri/shell-icon';
import { applyTheme, loadStoredTheme } from '@/lib/theme';
import { isTauriApp } from '@/lib/platform';
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
  const backend = getBackend();
  // fire-and-forget：失败写日志，不抛到 splash
  void loadAgentStatuses(backend).catch((e) => {
    log.error('agent status load failed', e);
  });
  void loadConnectionPool(backend)
    .then(() => reconcileAccountPool())
    .catch((e) => {
      log.error('connection pool load failed', e);
    });
  void loadTicketWallet(backend).catch((e) => {
    log.error('ticket wallet load failed', e);
  });
  void fetchCatalogShared().catch((e) => {
    log.error('skill catalog preload failed', e);
  });
  const lastProjectAgent = rememberedProjectAgent();
  if (lastProjectAgent) {
    void fetchAgentProjectsShared(lastProjectAgent, false).catch((e) => {
      log.error('project list preload failed', e);
    });
  }

  return loadAgentCatalog(backend)
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
  // 首屏前同步主题和主色,避免闪烁
  applyTheme(loadStoredTheme());
  applyAccent(loadStoredAccent());
  applyLanguage(loadStoredLanguage());
  if (isTauriApp()) {
    registerShellIconSync(applyShellAccentIconBestEffort);
    applyShellAccentIconBestEffort(loadStoredAccent());
  }

  // 立刻挂载 React：由 Root 内 splash 覆盖预加载，不再阻塞在 createRoot 之前
  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <AppErrorBoundary>
        <Root />
      </AppErrorBoundary>
    </React.StrictMode>,
  );
}

void boot();
