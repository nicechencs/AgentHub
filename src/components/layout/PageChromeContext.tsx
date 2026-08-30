import {
  createContext,
  useCallback,
  useContext,
  useLayoutEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

/** 非对话页顶栏要展示的页身份（标题 + 一行说明）。 */
export type PageChrome = {
  title: string;
  description?: string;
  descriptionTip?: string;
  badge?: ReactNode;
};

type PageChromeSetters = {
  setChrome: (next: PageChrome) => void;
  clearChrome: () => void;
};

const PageChromeStateContext = createContext<PageChrome | null>(null);
const PageChromeSettersContext = createContext<PageChromeSetters | null>(null);

export function PageChromeProvider({ children }: { children: ReactNode }) {
  const [chrome, setChromeState] = useState<PageChrome | null>(null);
  const setChrome = useCallback((next: PageChrome) => {
    setChromeState(next);
  }, []);
  const clearChrome = useCallback(() => {
    setChromeState(null);
  }, []);
  const setters = useMemo(
    () => ({ setChrome, clearChrome }),
    [setChrome, clearChrome],
  );

  return (
    <PageChromeSettersContext.Provider value={setters}>
      <PageChromeStateContext.Provider value={chrome}>{children}</PageChromeStateContext.Provider>
    </PageChromeSettersContext.Provider>
  );
}

export function usePageChrome(): PageChrome | null {
  return useContext(PageChromeStateContext);
}

/** 由 PageHeader 在绘制前登记顶栏文案；无 Provider 时为 no-op（测试可单挂页面）。 */
export function useRegisterPageChrome(chrome: PageChrome): void {
  const setters = useContext(PageChromeSettersContext);
  const { title, description, descriptionTip, badge } = chrome;

  useLayoutEffect(() => {
    if (!setters) return;
    setters.setChrome({ title, description, descriptionTip, badge });
    return () => setters.clearChrome();
  }, [setters, title, description, descriptionTip, badge]);
}
