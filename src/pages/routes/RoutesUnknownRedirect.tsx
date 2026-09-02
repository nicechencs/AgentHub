import { Navigate, useLocation } from 'react-router-dom';
import { routesIndexRedirectTo } from '@/lib/routes-path';

/** `/routes` index and unknown `/routes/*` → board, or auth pool when `?profile=`. */
export function RoutesIndexRedirect() {
  const { search } = useLocation();
  return <Navigate to={routesIndexRedirectTo(search)} replace />;
}

export function RoutesUnknownRedirect() {
  return <RoutesIndexRedirect />;
}
