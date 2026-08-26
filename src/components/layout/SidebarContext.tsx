import * as React from 'react';
import { loadBool, saveBool, StorageKey } from '@/lib/ui-preferences';

interface SidebarContextValue {
  collapsed: boolean;
  setCollapsed: (v: boolean) => void;
  toggle: () => void;
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
  const [collapsed, setCollapsedState] = React.useState(
    () => loadBool(StorageKey.sidebarCollapsed, false),
  );
  const [routesNavVisible, setRoutesNavVisibleState] = React.useState(
    () => loadBool(StorageKey.routesNavVisible, true),
  );
  const [pluginsNavVisible, setPluginsNavVisibleState] = React.useState(
    () => loadBool(StorageKey.pluginsNavVisible, true),
  );

  const setCollapsed = React.useCallback((v: boolean) => {
    setCollapsedState(v);
    saveBool(StorageKey.sidebarCollapsed, v);
  }, []);

  const setRoutesNavVisible = React.useCallback((v: boolean) => {
    setRoutesNavVisibleState(v);
    saveBool(StorageKey.routesNavVisible, v);
  }, []);

  const setPluginsNavVisible = React.useCallback((v: boolean) => {
    setPluginsNavVisibleState(v);
    saveBool(StorageKey.pluginsNavVisible, v);
  }, []);

  const toggle = React.useCallback(() => {
    setCollapsedState((prev) => {
      const next = !prev;
      saveBool(StorageKey.sidebarCollapsed, next);
      return next;
    });
  }, []);

  const value = React.useMemo(
    () => ({
      collapsed,
      setCollapsed,
      toggle,
      routesNavVisible,
      setRoutesNavVisible,
      pluginsNavVisible,
      setPluginsNavVisible,
    }),
    [
      collapsed,
      setCollapsed,
      toggle,
      routesNavVisible,
      setRoutesNavVisible,
      pluginsNavVisible,
      setPluginsNavVisible,
    ],
  );

  return <SidebarContext.Provider value={value}>{children}</SidebarContext.Provider>;
}
