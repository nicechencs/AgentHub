import * as React from 'react';
import { loadBool, saveBool, StorageKey } from '@/lib/storage';

interface SidebarContextValue {
  collapsed: boolean;
  setCollapsed: (v: boolean) => void;
  toggle: () => void;
}

const SidebarContext = React.createContext<SidebarContextValue>({
  collapsed: false,
  setCollapsed: () => {},
  toggle: () => {},
});

export function useSidebar() {
  return React.useContext(SidebarContext);
}

/** 侧栏折叠状态(持久化到 localStorage) */
export function SidebarProvider({ children }: { children: React.ReactNode }) {
  const [collapsed, setCollapsedState] = React.useState(
    () => loadBool(StorageKey.sidebarCollapsed, false),
  );

  const setCollapsed = React.useCallback((v: boolean) => {
    setCollapsedState(v);
    saveBool(StorageKey.sidebarCollapsed, v);
  }, []);

  const toggle = React.useCallback(() => {
    setCollapsedState((prev) => {
      const next = !prev;
      saveBool(StorageKey.sidebarCollapsed, next);
      return next;
    });
  }, []);

  const value = React.useMemo(
    () => ({ collapsed, setCollapsed, toggle }),
    [collapsed, setCollapsed, toggle],
  );

  return <SidebarContext.Provider value={value}>{children}</SidebarContext.Provider>;
}
