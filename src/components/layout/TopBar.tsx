import { NotificationBell } from '@/components/shared/NotificationBell';

/** 顶栏:仅右侧操作区（面包屑已删，避免与 PageHeader 重复） */
export function TopBar() {
  return (
    <header className="flex h-10 shrink-0 items-center justify-end border-b border-border bg-panel px-6">
      <div className="flex items-center gap-2">
        <NotificationBell />
      </div>
    </header>
  );
}
