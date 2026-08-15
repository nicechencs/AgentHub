import { Navigate, useLocation } from 'react-router-dom';
import { legacyBridgesRedirectTo } from '@/pages/adapter/adapter-model';

/** 兼容旧路径：/router → /bridges */
export default function RouterToAdapterRedirect() {
  const { search } = useLocation();
  return <Navigate to={legacyBridgesRedirectTo(search)} replace />;
}
