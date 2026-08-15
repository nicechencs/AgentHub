import { Navigate, useLocation } from 'react-router-dom';
import { legacyBridgesRedirectTo } from '@/lib/bridges-path';

/** 兼容旧路径：/router → /bridges */
export default function RouterToAdapterRedirect() {
  const { search } = useLocation();
  return <Navigate to={legacyBridgesRedirectTo(search)} replace />;
}
