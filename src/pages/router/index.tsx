import { Navigate } from 'react-router-dom';

/** 兼容旧路径：Router 已更名为 Adapter */
export default function RouterToAdapterRedirect() {
  return <Navigate to="/adapter" replace />;
}
