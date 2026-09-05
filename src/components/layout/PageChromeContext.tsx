import {
  createContext,
  useCallback,
  useContext,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import {
  removePageChromeEntry,
  topPageChrome,
  upsertPageChromeEntry,
  type PageChromeEntry,
} from '@/components/layout/page-chrome-model';

/** 非对话页顶栏要展示的页身份（标题 + 一行说明）。 */
export type PageChrome = {
  title: string;
  description?: string;
  descriptionTip?: string;
  badge?: ReactNode;
};

type PageChromeSetters = {
  upsert: (id: number, next: PageChrome) => void;
  remove: (id: number) => void;
};

const PageChromeStateContext = createContext<PageChrome | null>(null);
const PageChromeSettersContext = createContext<PageChromeSetters | null>(null);

let nextChromeId = 1;

export function PageChromeProvider({ children }: { children: ReactNode }) {
  const [entries, setEntries] = useState<PageChromeEntry<PageChrome>[]>([]);
  const upsert = useCallback((id: number, next: PageChrome) => {
    setEntries((prev) => upsertPageChromeEntry(prev, id, next));
  }, []);
  const remove = useCallback((id: number) => {
    setEntries((prev) => removePageChromeEntry(prev, id));
  }, []);
  const setters = useMemo(() => ({ upsert, remove }), [upsert, remove]);
  const chrome = topPageChrome(entries);

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
  const idRef = useRef(0);
  if (idRef.current === 0) idRef.current = nextChromeId++;
  const { title, description, descriptionTip, badge } = chrome;

  useLayoutEffect(() => {
    if (!setters) return;
    const id = idRef.current;
    setters.upsert(id, { title, description, descriptionTip, badge });
    return () => setters.remove(id);
  }, [setters, title, description, descriptionTip, badge]);
}
