import { Navigate, useLocation } from 'react-router-dom';
import { BRIDGES_PATH } from '@/lib/bridges-path';

/** Unknown `/routes/*` → list; preserve safe query (e.g. ?profile=). */
export function RoutesUnknownRedirect() {
  const { search } = useLocation();
  return <Navigate to={`${BRIDGES_PATH}${search}`} replace />;
}
