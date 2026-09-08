import * as React from 'react';
import {
  DEFAULT_NAV_VISIBILITY,
  DEFAULT_SIDEBAR_AUTO_COLLAPSE_ON_ROUTES,
  loadBool,
  OPTIONAL_NAV_IDS,
  OPTIONAL_NAV_STORAGE_KEY,
  saveBool,
  StorageKey,
  type NavVisibility,
  type OptionalNavId,
} from '@/lib/ui-preferences';

function loadNavVisibility(): NavVisibility {
  const next = { ...DEFAULT_NAV_VISIBILITY };
  for (const id of OPTIONAL_NAV_IDS) {
    next[id] = loadBool(OPTIONAL_NAV_STORAGE_KEY[id], DEFAULT_NAV_VISIBILITY[id]);
  }
  return next;
}

interface SidebarContextValue {
  collapsed: boolean;
  setCollapsed: (v: boolean) => void;
  toggle: () => void;
  /** Secondary-nav control: expand the primary sidebar (persisted). */
  expandPrimarySidebar: () => void;
  /** When on, clicking Routes in the primary nav collapses it. */
  autoCollapseOnRoutes: boolean;
  setAutoCollapseOnRoutes: (v: boolean) => void;
  navVisible: NavVisibility;
  setNavVisible: (id: OptionalNavId, v: boolean) => void;
  routesNavVisible: boolean;
  setRoutesNavVisible: (v: boolean) => void;
  pluginsNavVisible: boolean;
  setPluginsNavVisible: (v: boolean) => void;
  sub2apiNavVisible: boolean;
  setSub2apiNavVisible: (v: boolean) => void;
}

const SidebarContext = React.createContext<SidebarContextValue | undefined>(undefined);

export function useSidebar() {
  const value = React.useContext(SidebarContext);
  if (!value) {
    throw new Error('SidebarProvider is required');
  }
  return value;
}

/** 侧栏 UI 偏好（折叠、路由点击自动折叠、可选入口可见性；持久化到 localStorage） */
export function SidebarProvider({ children }: { children: React.ReactNode }) {
  const [collapsed, setCollapsedState] = React.useState(
    () => loadBool(StorageKey.sidebarCollapsed, false),
  );
  const [autoCollapseOnRoutes, setAutoCollapseOnRoutesState] = React.useState(
    () =>
      loadBool(StorageKey.sidebarAutoCollapseOnRoutes, DEFAULT_SIDEBAR_AUTO_COLLAPSE_ON_ROUTES),
  );
  const [navVisible, setNavVisibleState] = React.useState(loadNavVisibility);

  const setCollapsed = React.useCallback((v: boolean) => {
    setCollapsedState(v);
    saveBool(StorageKey.sidebarCollapsed, v);
  }, []);

  const setAutoCollapseOnRoutes = React.useCallback((v: boolean) => {
    setAutoCollapseOnRoutesState(v);
    saveBool(StorageKey.sidebarAutoCollapseOnRoutes, v);
  }, []);

  const setNavVisible = React.useCallback((id: OptionalNavId, v: boolean) => {
    setNavVisibleState((prev) => ({ ...prev, [id]: v }));
    saveBool(OPTIONAL_NAV_STORAGE_KEY[id], v);
  }, []);

  const setRoutesNavVisible = React.useCallback(
    (v: boolean) => setNavVisible('routes', v),
    [setNavVisible],
  );
  const setPluginsNavVisible = React.useCallback(
    (v: boolean) => setNavVisible('plugins', v),
    [setNavVisible],
  );
  const setSub2apiNavVisible = React.useCallback(
    (v: boolean) => setNavVisible('sub2api', v),
    [setNavVisible],
  );

  const toggle = React.useCallback(() => {
    setCollapsedState((prev) => {
      const next = !prev;
      saveBool(StorageKey.sidebarCollapsed, next);
      return next;
    });
  }, []);

  const expandPrimarySidebar = React.useCallback(() => {
    setCollapsed(false);
  }, [setCollapsed]);

  const value = React.useMemo(
    () => ({
      collapsed,
      setCollapsed,
      toggle,
      expandPrimarySidebar,
      autoCollapseOnRoutes,
      setAutoCollapseOnRoutes,
      navVisible,
      setNavVisible,
      routesNavVisible: navVisible.routes,
      setRoutesNavVisible,
      pluginsNavVisible: navVisible.plugins,
      setPluginsNavVisible,
      sub2apiNavVisible: navVisible.sub2api,
      setSub2apiNavVisible,
    }),
    [
      collapsed,
      setCollapsed,
      toggle,
      expandPrimarySidebar,
      autoCollapseOnRoutes,
      setAutoCollapseOnRoutes,
      navVisible,
      setNavVisible,
      setRoutesNavVisible,
      setPluginsNavVisible,
      setSub2apiNavVisible,
    ],
  );

  return <SidebarContext.Provider value={value}>{children}</SidebarContext.Provider>;
}
