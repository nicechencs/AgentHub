import { Navigate, Outlet, useLocation } from 'react-router-dom';
import { isRoutesAreaPath } from '@/pages/routes/routes-nav-items';

/**
 * 路由区外壳：子路由出口。一级侧栏折叠不在此自动改（点「路由」且设置打开时才收起）。
 * 二级导航由 App 在 shell 级渲染。
 */
export function RoutesLayout() {
  const { pathname } = useLocation();

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
