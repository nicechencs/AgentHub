import { useCallback, useRef } from 'react';
import { Navigate, Route, Routes, useLocation } from 'react-router-dom';
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
import RouterPage from '@/pages/router';
import SkillsPage from '@/pages/skills';
import McpPage from '@/pages/mcp';
import ProjectsPage from '@/pages/projects';
import SettingsPage from '@/pages/settings';
import {
  checkForUpdate,
  isUpdateAvailable,
  type UpdateInfo,
} from '@/lib/api/update';
import { cn } from '@/lib/utils';

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

/** 独立 Backups 页已并入 Settings → /settings?tab=backups */
function LegacyBackupsRedirect() {
  return <Navigate to="/settings?tab=backups" replace />;
}

export default function App() {
  const { pathname } = useLocation();
  const isChat = pathname === '/chat';
  /** 技能页需左右分栏铺满主区，不受 max-w-content 限制 */
  const isSkills = pathname === '/skills';
  const fullBleed = isChat || isSkills;
  const updateHandleRef = useRef<UpdatePromptHandle | null>(null);

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
      throw new Error('仅桌面端支持自动更新');
    }
    return checkForUpdate();
  }, []);

  return (
    <SidebarProvider>
      <div className="flex h-full bg-canvas">
        <Sidebar />
        <div className="flex min-w-0 flex-1 flex-col">
          {!isChat && <TopBar />}
          <main className={cn('flex-1', fullBleed ? 'overflow-hidden' : 'overflow-y-auto')}>
            {/* max-w-content：普通页桌面阅读宽度；chat/skills 全宽全高 */}
            <div className={fullBleed ? 'h-full' : pageRhythm.pageShell}>
              <Routes>
                <Route path="/" element={<DashboardPage />} />
                <Route path="/chat" element={<ChatPage />} />
                <Route path="/agents" element={<AgentsPage />} />
                <Route path="/connections" element={<ConnectionsPage />} />
                <Route path="/router" element={<RouterPage />} />
                {/* 兼容旧路由与深链 */}
                <Route path="/providers" element={<LegacyConnectionsRedirect mode="providers" />} />
                <Route path="/accounts" element={<LegacyConnectionsRedirect mode="accounts" />} />
                <Route path="/skills" element={<SkillsPage />} />
                <Route path="/mcp" element={<McpPage />} />
                <Route path="/projects" element={<ProjectsPage />} />
                <Route path="/usage" element={<LegacyUsageRedirect />} />
                {/* 已并入 Settings */}
                <Route path="/backups" element={<LegacyBackupsRedirect />} />
                <Route
                  path="/settings"
                  element={<SettingsPage onCheckUpdate={checkForAppUpdate} />}
                />
              </Routes>
            </div>
          </main>
        </div>
        <OnboardingDialog />
        <UpdatePrompt onReady={onUpdateReady} />
      </div>
    </SidebarProvider>
  );
}
