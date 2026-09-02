import { Navigate, useLocation } from 'react-router-dom';
import { legacyBridgesRedirectTo } from '@/lib/routes-path';

/** 兼容旧路径：/router → /routes */
export default function RouterToAdapterRedirect() {
  const { search } = useLocation();
  return <Navigate to={legacyBridgesRedirectTo(search)} replace />;
}
