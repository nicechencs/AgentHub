import * as React from 'react';
import { loadBool, saveBool, StorageKey } from '@/lib/ui-preferences';

interface SidebarContextValue {
  collapsed: boolean;
  setCollapsed: (v: boolean) => void;
  toggle: () => void;
  routesNavVisible: boolean;
  setRoutesNavVisible: (v: boolean) => void;
}

const SidebarContext = React.createContext<SidebarContextValue>({
  collapsed: false,
  setCollapsed: () => {},
  toggle: () => {},
  routesNavVisible: true,
  setRoutesNavVisible: () => {},
});

export function useSidebar() {
  return React.useContext(SidebarContext);
}

/** 侧栏 UI 偏好（折叠、路由入口可见性；持久化到 localStorage） */
export function SidebarProvider({ children }: { children: React.ReactNode }) {
  const [collapsed, setCollapsedState] = React.useState(
    () => loadBool(StorageKey.sidebarCollapsed, false),
  );
  const [routesNavVisible, setRoutesNavVisibleState] = React.useState(
    () => loadBool(StorageKey.routesNavVisible, true),
  );

  const setCollapsed = React.useCallback((v: boolean) => {
    setCollapsedState(v);
    saveBool(StorageKey.sidebarCollapsed, v);
  }, []);

  const setRoutesNavVisible = React.useCallback((v: boolean) => {
    setRoutesNavVisibleState(v);
    saveBool(StorageKey.routesNavVisible, v);
  }, []);

  const toggle = React.useCallback(() => {
    setCollapsedState((prev) => {
      const next = !prev;
      saveBool(StorageKey.sidebarCollapsed, next);
      return next;
    });
  }, []);

  const value = React.useMemo(
    () => ({ collapsed, setCollapsed, toggle, routesNavVisible, setRoutesNavVisible }),
    [collapsed, setCollapsed, toggle, routesNavVisible, setRoutesNavVisible],
  );

  return <SidebarContext.Provider value={value}>{children}</SidebarContext.Provider>;
}
