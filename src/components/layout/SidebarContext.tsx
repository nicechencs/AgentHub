import * as React from 'react';
import {
  DEFAULT_PLUGINS_NAV_VISIBLE,
  DEFAULT_ROUTES_NAV_VISIBLE,
  loadBool,
  saveBool,
  StorageKey,
} from '@/lib/ui-preferences';
import {
  effectiveCollapsed,
  onEnterRoutesArea,
  onExpandPrimaryFromRoutes,
  onLeaveRoutesArea,
  onToggleInRoutesArea,
} from '@/components/layout/sidebar-collapse-override';

interface SidebarContextValue {
  /** Effective collapsed (session override wins over stored preference). */
  collapsed: boolean;
  setCollapsed: (v: boolean) => void;
  toggle: () => void;
  /** True while a `/routes*` page is mounted. */
  routesAreaActive: boolean;
  enterRoutesArea: () => void;
  leaveRoutesArea: () => void;
  /** Secondary-nav control: expand primary sidebar for this session only. */
  expandPrimarySidebar: () => void;
  routesNavVisible: boolean;
  setRoutesNavVisible: (v: boolean) => void;
  pluginsNavVisible: boolean;
  setPluginsNavVisible: (v: boolean) => void;
}

const SidebarContext = React.createContext<SidebarContextValue | undefined>(undefined);

export function useSidebar() {
  const value = React.useContext(SidebarContext);
  if (!value) {
    throw new Error('SidebarProvider is required');
  }
  return value;
}

/** 侧栏 UI 偏好（折叠、路由/插件入口可见性；持久化到 localStorage） */
export function SidebarProvider({ children }: { children: React.ReactNode }) {
  const [storedCollapsed, setStoredCollapsed] = React.useState(
    () => loadBool(StorageKey.sidebarCollapsed, false),
  );
  const [sessionCollapsed, setSessionCollapsed] = React.useState<boolean | null>(null);
  const [routesAreaActive, setRoutesAreaActive] = React.useState(false);
  const [routesNavVisible, setRoutesNavVisibleState] = React.useState(
    () => loadBool(StorageKey.routesNavVisible, DEFAULT_ROUTES_NAV_VISIBLE),
  );
  const [pluginsNavVisible, setPluginsNavVisibleState] = React.useState(
    () => loadBool(StorageKey.pluginsNavVisible, DEFAULT_PLUGINS_NAV_VISIBLE),
  );

  const collapsed = effectiveCollapsed({
    stored: storedCollapsed,
    session: sessionCollapsed,
  });

  const setCollapsed = React.useCallback((v: boolean) => {
    if (sessionCollapsed !== null) {
      setSessionCollapsed(v);
      return;
    }
    setStoredCollapsed(v);
    saveBool(StorageKey.sidebarCollapsed, v);
  }, [sessionCollapsed]);

  const setRoutesNavVisible = React.useCallback((v: boolean) => {
    setRoutesNavVisibleState(v);
    saveBool(StorageKey.routesNavVisible, v);
  }, []);

  const setPluginsNavVisible = React.useCallback((v: boolean) => {
    setPluginsNavVisibleState(v);
    saveBool(StorageKey.pluginsNavVisible, v);
  }, []);

  const toggle = React.useCallback(() => {
    setSessionCollapsed((session) => {
      if (session === null) {
        setStoredCollapsed((prev) => {
          const next = !prev;
          saveBool(StorageKey.sidebarCollapsed, next);
          return next;
        });
        return null;
      }
      const next = onToggleInRoutesArea({ stored: storedCollapsed, session });
      return next.session;
    });
  }, [storedCollapsed]);

  const enterRoutesArea = React.useCallback(() => {
    setRoutesAreaActive(true);
    setSessionCollapsed((session) => {
      const next = onEnterRoutesArea({ stored: storedCollapsed, session });
      return next.session;
    });
  }, [storedCollapsed]);

  const leaveRoutesArea = React.useCallback(() => {
    setRoutesAreaActive(false);
    setSessionCollapsed((session) => onLeaveRoutesArea({ stored: storedCollapsed, session }).session);
  }, [storedCollapsed]);

  const expandPrimarySidebar = React.useCallback(() => {
    setSessionCollapsed((session) =>
      onExpandPrimaryFromRoutes({ stored: storedCollapsed, session }).session,
    );
  }, [storedCollapsed]);

  const value = React.useMemo(
    () => ({
      collapsed,
      setCollapsed,
      toggle,
      routesAreaActive,
      enterRoutesArea,
      leaveRoutesArea,
      expandPrimarySidebar,
      routesNavVisible,
      setRoutesNavVisible,
      pluginsNavVisible,
      setPluginsNavVisible,
    }),
    [
      collapsed,
      setCollapsed,
      toggle,
      routesAreaActive,
      enterRoutesArea,
      leaveRoutesArea,
      expandPrimarySidebar,
      routesNavVisible,
      setRoutesNavVisible,
      pluginsNavVisible,
      setPluginsNavVisible,
    ],
  );

  return <SidebarContext.Provider value={value}>{children}</SidebarContext.Provider>;
}
