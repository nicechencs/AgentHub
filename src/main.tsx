import React from 'react';
import ReactDOM from 'react-dom/client';
import { HashRouter } from 'react-router-dom';
import App from './App';
import { BootSplash } from '@/components/shared/BootSplash';
import { ToastProvider } from '@/components/ui/toast';
import { TooltipProvider } from '@/components/ui/tooltip';
import { ThemeProvider } from '@/components/shared/ThemeProvider';
import { UsageSyncProvider } from '@/components/shared/UsageSyncProvider';
import { AgentCatalogProvider, getBackend, loadAgentCatalog } from '@/app/runtime';
import { applyTheme, loadStoredTheme } from '@/lib/theme';
import { logger } from '@/lib/logger';
// Design tokens first (SSOT: src/styles/tokens.ts), then structural styles
import 'virtual:agenthub-design-tokens.css';
import '@/styles/globals.css';

const log = logger.scope('boot');

/** 最短展示时长：避免 hydrate 极快时 splash 一闪而过 */
const MIN_SPLASH_MS = 900;
/** React 挂载后稍等一帧再退场，让视觉接管更顺 */
const EXIT_DELAY_MS = 60;

// 首屏前同步主题,避免闪烁
applyTheme(loadStoredTheme());

function Root() {
  const [splashPhase, setSplashPhase] = React.useState<'show' | 'exit' | 'gone'>('show');

  React.useEffect(() => {
    const t = window.setTimeout(() => setSplashPhase('exit'), EXIT_DELAY_MS);
    return () => window.clearTimeout(t);
  }, []);

  const onExited = React.useCallback(() => setSplashPhase('gone'), []);

  return (
    <>
      {splashPhase !== 'gone' && (
        <BootSplash exiting={splashPhase === 'exit'} onExited={onExited} />
      )}
      <HashRouter>
        <ThemeProvider>
          <TooltipProvider delayDuration={200} skipDelayDuration={0}>
            <ToastProvider>
              <AgentCatalogProvider>
                <UsageSyncProvider>
                  <App />
                </UsageSyncProvider>
              </AgentCatalogProvider>
            </ToastProvider>
          </TooltipProvider>
        </ThemeProvider>
      </HashRouter>
    </>
  );
}

async function boot() {
  const started = performance.now();

  try {
    // Product agent set from backend catalog; on failure leave AGENTS empty (no static fallback).
    await loadAgentCatalog(getBackend());
  } catch (e) {
    log.error('agent catalog load failed', e);
  }

  const elapsed = performance.now() - started;
  if (elapsed < MIN_SPLASH_MS) {
    await new Promise<void>((resolve) => {
      window.setTimeout(resolve, MIN_SPLASH_MS - elapsed);
    });
  }

  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <Root />
    </React.StrictMode>,
  );
}

void boot();
