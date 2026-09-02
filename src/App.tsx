import { useCallback, useEffect, useRef } from 'react';
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom';
import { PageChromeProvider } from '@/components/layout/PageChromeContext';
import { Sidebar } from '@/components/layout/Sidebar';
import { SidebarProvider } from '@/components/layout/SidebarContext';
import { TopBar } from '@/components/layout/TopBar';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { OnboardingDialog } from '@/components/shared/OnboardingDialog';
import { UpdatePrompt, type UpdatePromptHandle } from '@/components/shared/UpdatePrompt';
import DashboardPage from '@/pages/dashboard';
import ChatPage from '@/pages/chat';
import AgentsPage from '@/pages/agents';
import ConnectionsPage from '@/pages/connections';
import SkillsPage from '@/pages/skills';
import McpPage from '@/pages/mcp';
import PluginsPage from '@/pages/plugins';
import ProjectsPage from '@/pages/projects';
import SettingsPage from '@/pages/settings';
import { RoutesLayout } from '@/pages/routes/RoutesLayout';
import { RoutesNav } from '@/pages/routes/RoutesNav';
import { RoutesIndexRedirect, RoutesUnknownRedirect } from '@/pages/routes/RoutesUnknownRedirect';
import RoutesBoardPage from '@/pages/routes/board';
import RoutesPoolPage from '@/pages/routes/pool';
import RoutesTokensPage from '@/pages/routes/tokens';
import RoutesActivityPage from '@/pages/routes/activity';
import { isRoutesAreaPath } from '@/pages/routes/routes-nav-items';
import { onTrayNavigate } from '@/lib/backend/tauri/tray-events';
import { legacyBridgesRedirectTo } from '@/lib/bridges-path';
import {
  checkForUpdate,
  isUpdateAvailable,
  type UpdateInfo,
} from '@/lib/api/update';
import { useI18n } from '@/components/shared/LanguageProvider';
import { cn } from '@/lib/utils';
import { logger } from '@/lib/logger';

/** 旧 /providers、/accounts 深链兼容 → /connections */
function LegacyConnectionsRedirect({ mode }: { mode: 'providers' | 'accounts' }) {
  const { search } = useLocation();
  const params = new URLSearchParams(search);
  if (mode === 'providers') params.set('mode', 'providers');
  else params.delete('mode');
  const qs = params.toString();
  return <Navigate to={qs ? `/connections?${qs}` : '/connections'} replace />;
}

/** 独立 Usage 页已合并进 Dashboard → /?section=usage */
function LegacyUsageRedirect() {
  return <Navigate to="/?section=usage" replace />;
}

/** 旧 /adapter、/router、/bridges 深链兼容 → 看板，或带 profile 时到连接池 */
function LegacyBridgesRedirect() {
  const { search } = useLocation();
  return <Navigate to={legacyBridgesRedirectTo(search)} replace />;
}

/** 独立 Backups 页已并入 Settings → /settings?tab=backups */
function LegacyBackupsRedirect() {
  return <Navigate to="/settings?tab=backups" replace />;
}

export default function App() {
  const { t } = useI18n();
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const isChat = pathname === '/chat';
  const isRoutesArea = isRoutesAreaPath(pathname);
  /** Skills / Projects / Connections / Routes / Agents / Plugins / Settings 左右分栏需要全高 overflow-hidden，不套 pageShell 内边距 */
  const isWorkbenchSplit =
    pathname === '/skills' ||
    pathname === '/projects' ||
    pathname === '/connections' ||
    isRoutesArea ||
    pathname === '/agents' ||
    pathname === '/plugins' ||
    pathname === '/settings';
  const fullBleed = isChat || isWorkbenchSplit;
  const updateHandleRef = useRef<UpdatePromptHandle | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void onTrayNavigate((path) => {
      navigate(path);
    }).then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unsub = fn;
    }).catch((error) => {
      // Tray navigation is Tauri-only. In browser/mock mode this rejection is
      // expected; in production it must remain fail-closed rather than
      // installing a mock listener or turning into an unhandled rejection.
      logger.scope('tray').error('tray navigation subscription unavailable', error);
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [navigate]);

  const onUpdateReady = useCallback((handle: UpdatePromptHandle) => {
    updateHandleRef.current = handle;
  }, []);

  const checkForAppUpdate = useCallback(async (): Promise<UpdateInfo | null> => {
    // Prefer shared dialog path once UpdatePrompt has registered.
    // quietWhenCurrent: settings page owns the “已是最新” toast.
    if (updateHandleRef.current) {
      return updateHandleRef.current.checkNow({ quietWhenCurrent: true });
    }
    // Fallback: settings opened before prompt mounted (should be rare).
    if (!(await isUpdateAvailable())) {
      throw new Error(t('settings.page.desktopOnlyUpdate'));
    }
    return checkForUpdate();
  }, [t]);

  return (
    <SidebarProvider>
      <div className={pageRhythm.shell}>
        <Sidebar />
        {isRoutesArea ? <RoutesNav /> : null}
        <PageChromeProvider>
          <div className={pageRhythm.shellMain}>
            {!isChat && <TopBar />}
            <main
              className={cn(
                'min-h-0 flex-1',
                fullBleed ? 'overflow-hidden' : 'overflow-y-auto',
              )}
            >
              {/* 常规页铺满主列 + pageRhythm.pageShell；chat 与左右分栏工作台全高自管 */}
              <div className={fullBleed ? 'h-full min-h-0' : pageRhythm.pageShell}>
                <Routes>
                  <Route path="/" element={<DashboardPage />} />
                  <Route path="/chat" element={<ChatPage />} />
                  <Route path="/agents" element={<AgentsPage />} />
                  <Route path="/connections" element={<ConnectionsPage />} />
                  <Route path="/routes" element={<RoutesLayout />}>
                    <Route index element={<RoutesIndexRedirect />} />
                    <Route path="board" element={<RoutesBoardPage />} />
                    <Route path="pool" element={<RoutesPoolPage />} />
                    <Route path="tokens" element={<RoutesTokensPage />} />
                    <Route path="activity" element={<RoutesActivityPage />} />
                    <Route path="*" element={<RoutesUnknownRedirect />} />
                  </Route>
                  <Route path="/bridges" element={<LegacyBridgesRedirect />} />
                  <Route path="/adapter" element={<LegacyBridgesRedirect />} />
                  <Route path="/router" element={<LegacyBridgesRedirect />} />
                  {/* 兼容旧路由与深链 */}
                  <Route path="/providers" element={<LegacyConnectionsRedirect mode="providers" />} />
                  <Route path="/accounts" element={<LegacyConnectionsRedirect mode="accounts" />} />
                  <Route path="/skills" element={<SkillsPage />} />
                  <Route path="/mcp" element={<McpPage />} />
                  <Route path="/projects" element={<ProjectsPage />} />
                  <Route path="/plugins" element={<PluginsPage />} />
                  <Route path="/usage" element={<LegacyUsageRedirect />} />
                  <Route path="/backups" element={<LegacyBackupsRedirect />} />
                  <Route
                    path="/settings"
                    element={<SettingsPage onCheckUpdate={checkForAppUpdate} />}
                  />
                </Routes>
              </div>
            </main>
          </div>
        </PageChromeProvider>
        <OnboardingDialog />
        <UpdatePrompt onReady={onUpdateReady} />
      </div>
    </SidebarProvider>
  );
}
