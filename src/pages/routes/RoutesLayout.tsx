import { useLayoutEffect } from 'react';
import { Navigate, Outlet, useLocation } from 'react-router-dom';
import { useSidebar } from '@/components/layout/SidebarContext';
import { isRoutesAreaPath } from '@/pages/routes/routes-nav-items';

/**
 * 路由区外壳：挂载时会话级折叠一级侧栏；卸载时恢复持久偏好。
 * 二级导航由 App 在 shell 级渲染；本组件只负责副作用与子路由出口。
 * 使用 useLayoutEffect，避免首帧先展开再折叠的闪烁，以及展开按钮抢跑 enter。
 */
export function RoutesLayout() {
  const { pathname } = useLocation();
  const { enterRoutesArea, leaveRoutesArea } = useSidebar();

  useLayoutEffect(() => {
    enterRoutesArea();
    return () => {
      leaveRoutesArea();
    };
  }, [enterRoutesArea, leaveRoutesArea]);

  if (!isRoutesAreaPath(pathname)) {
    return <Navigate to="/routes" replace />;
  }

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col" data-routes-layout>
      <Outlet />
    </div>
  );
}

export default RoutesLayout;
