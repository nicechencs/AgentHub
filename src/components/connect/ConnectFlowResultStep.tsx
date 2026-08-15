import { Notice } from '@/components/shared/Notice';
import { connectFlowResultMessage, type ConnectFlowState } from './connect-flow-state';

export function ConnectFlowResultStep({ result }: { result: NonNullable<ConnectFlowState['result']> }) {
  const message = connectFlowResultMessage(result);
  const tone = result.kind === 'failed' ? 'danger' : result.refreshFailed ? 'warning' : 'success';
  return (
    <Notice tone={tone}>
      <p className="text-sm font-medium text-primary">{message}</p>
      {result.kind === 'applied' && result.isCurrent && !result.refreshFailed ? (
        <p className="mt-1">目标 Agent 已使用新的连接。</p>
      ) : null}
      {result.kind === 'failed' ? (
        <p className="mt-1">选择与预览仍保留，可直接重试。</p>
      ) : null}
    </Notice>
  );
}
