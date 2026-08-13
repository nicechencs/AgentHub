import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { Skeleton } from '@/components/ui/skeleton';
import {
  ColumnResizeHandle,
  Table,
  TableBody,
  TableCell,
  TableFooterBar,
  TableHead,
  TableHeader,
  TableHeaderRow,
  TableRow,
  TableShell,
  useColumnWidths,
  type ColumnWidthSpec,
} from '@/components/ui/table';
import type {
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import { AdapterErrorLines, isBridgeStopCapable } from './adapter-components';
import {
  adapterBridgeUpstreamLabel,
  adapterCredentialKindLabel,
  adapterProfileRecordLabel,
  adapterTableRouteLabel,
  profileStatusBadge,
  targetAgentName,
} from './adapter-model';
import {
  adapterFailurePresentation,
  adapterNeedsAttentionRecovery,
  adapterProfileLastErrorCode,
  adapterProfilePortLabel,
} from './adapter-sources';

type ColumnKey = 'source' | 'target' | 'credential' | 'route' | 'status' | 'endpoint' | 'actions';

const WIDTH_SPECS: ColumnWidthSpec<ColumnKey>[] = [
  { key: 'source', defaultWidth: 200, minWidth: 132 },
  { key: 'target', defaultWidth: 112, minWidth: 80 },
  { key: 'credential', defaultWidth: 96, minWidth: 80 },
  { key: 'route', defaultWidth: 128, minWidth: 88 },
  { key: 'status', defaultWidth: 120, minWidth: 80 },
  { key: 'endpoint', defaultWidth: 200, minWidth: 132 },
  { key: 'actions', defaultWidth: 220, minWidth: 168 },
];

const COLUMN_LABELS: Record<ColumnKey, string> = {
  source: '来源',
  target: '目标',
  credential: '凭据类型',
  route: '路径',
  status: '状态',
  endpoint: '端点 / Provider',
  actions: '操作',
};

export type AdapterProfilesTableProps = {
  profiles: AdapterProfile[];
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
  loading: boolean;
  loadError: unknown;
  errors: Record<string, unknown>;
  removingProfileId: string | null;
  busyProfileIds: Record<string, boolean>;
  onRemove: (profile: AdapterProfile) => void;
  onStartBridge: (profile: AdapterProfile) => void;
  onRequestStopBridge: (profile: AdapterProfile) => void;
  onSetBridgeAutoStart: (profile: AdapterProfile, autoStart: boolean) => void;
  onRetry: () => void;
};

export function AdapterProfilesTable({
  profiles,
  bridgeStatuses,
  loading,
  loadError,
  errors,
  removingProfileId,
  busyProfileIds,
  onRemove,
  onStartBridge,
  onRequestStopBridge,
  onSetBridgeAutoStart,
  onRetry,
}: AdapterProfilesTableProps) {
  const { widths, onResizeStart, totalWidth } = useColumnWidths(WIDTH_SPECS);

  return (
    <TableShell
      footer={(
        <TableFooterBar>
          <p>已创建 {profiles.length} 条适配</p>
        </TableFooterBar>
      )}
    >
      <Table className="table-fixed" style={{ minWidth: totalWidth }}>
        <colgroup>
          {WIDTH_SPECS.map((column) => (
            <col key={column.key} style={{ width: widths[column.key] }} />
          ))}
        </colgroup>
        <TableHeader>
          <TableHeaderRow>
            {(Object.keys(COLUMN_LABELS) as ColumnKey[]).map((key) => (
              <TableHead key={key} className="relative select-none">
                {COLUMN_LABELS[key]}
                {typeof document !== 'undefined' ? (
                  <ColumnResizeHandle
                    columnKey={key}
                    label={COLUMN_LABELS[key]}
                    onResizeStart={onResizeStart}
                  />
                ) : null}
              </TableHead>
            ))}
          </TableHeaderRow>
        </TableHeader>
        <TableBody>
          {loading ? (
            <TableRow>
              <TableCell colSpan={7}><Skeleton className="h-8 w-full" /></TableCell>
            </TableRow>
          ) : loadError ? (
            <TableRow>
              <TableCell colSpan={7}>
                <div className="space-y-2" role="alert">
                  <AdapterErrorLines error={loadError} fallback="无法读取已创建的适配。" />
                  <Button variant="outline" size="sm" onClick={onRetry}>重试</Button>
                </div>
              </TableCell>
            </TableRow>
          ) : profiles.length === 0 ? (
            <TableRow>
              <TableCell colSpan={7}>
                <p className="text-sm text-secondary">尚未创建适配。从上方选择连接并预览路径后即可应用。</p>
              </TableCell>
            </TableRow>
          ) : profiles.map((profile) => {
            const status = profileStatusBadge(profile.status);
            const removing = removingProfileId === profile.id;
            const bridgeStatus = profile.route === 'local_bridge' ? bridgeStatuses[profile.id] : undefined;
            const busy = busyProfileIds[profile.id] === true;
            const bridgeTransitioning = bridgeStatus?.state === 'starting' || bridgeStatus?.state === 'stopping';
            const lastErrorCode = adapterProfileLastErrorCode(profile);
            const portLabel = adapterProfilePortLabel(profile, bridgeStatus);
            const recovery = profile.status === 'needs_attention'
              ? adapterNeedsAttentionRecovery(profile, bridgeStatus?.state)
              : null;
            const rowError = errors[profile.id]
              ? adapterFailurePresentation(errors[profile.id], '适配操作失败')
              : null;
            const showBridgeControls = profile.route === 'local_bridge';
            return (
              <TableRow key={profile.id}>
                <TableCell className="min-w-0">
                  <p className="truncate font-medium">{adapterProfileRecordLabel(profile)}</p>
                  {lastErrorCode ? (
                    <p className="mt-0.5 truncate text-xs text-secondary">lastErrorCode：{lastErrorCode}</p>
                  ) : null}
                </TableCell>
                <TableCell>{targetAgentName(profile.targetAgentId)}</TableCell>
                <TableCell>
                  <Badge variant="default">{adapterCredentialKindLabel(profile.mode)}</Badge>
                </TableCell>
                <TableCell>
                  <Badge variant="default">{adapterTableRouteLabel(profile.route)}</Badge>
                </TableCell>
                <TableCell>
                  <div className="flex flex-wrap items-center gap-1">
                    <Badge variant={status.variant}>{status.label}</Badge>
                    {showBridgeControls && bridgeStatus ? (
                      <Badge variant={bridgeStatus.state === 'running' ? 'success' : bridgeStatus.state === 'error' || bridgeStatus.state === 'degraded' ? 'warning' : 'default'}>
                        {bridgeStatus.state === 'running' ? '运行中' : bridgeStatus.state === 'error' ? '运行错误' : bridgeStatus.state === 'degraded' ? '服务降级' : '已停止'}
                      </Badge>
                    ) : null}
                  </div>
                </TableCell>
                <TableCell className="min-w-0 text-xs text-secondary">
                  <p>生成 Provider：{profile.generatedProviderId ?? '无'}</p>
                  {profile.route === 'local_bridge' || profile.localPort ? (
                    <p>端口：{portLabel}</p>
                  ) : null}
                  {bridgeStatus?.upstreamStatus ? (
                    <p>上游：{adapterBridgeUpstreamLabel(bridgeStatus.upstreamStatus)}</p>
                  ) : null}
                </TableCell>
                <TableCell>
                  <div className="flex flex-wrap items-center gap-2">
                    {showBridgeControls && (
                      <>
                        <label className="flex items-center gap-2 text-xs text-secondary">
                          <Switch
                            checked={profile.autoStart}
                            disabled={busy}
                            aria-label={`${adapterProfileRecordLabel(profile)} 自动启动`}
                            onCheckedChange={(autoStart) => onSetBridgeAutoStart(profile, autoStart)}
                          />
                          自动启动
                        </label>
                        {isBridgeStopCapable(bridgeStatus?.state) ? (
                          <Button
                            variant="outline"
                            size="sm"
                            disabled={busy || bridgeTransitioning}
                            onClick={() => onRequestStopBridge(profile)}
                          >
                            {busy ? '处理中…' : '停止'}
                          </Button>
                        ) : (
                          <Button
                            variant="outline"
                            size="sm"
                            disabled={busy || bridgeTransitioning}
                            onClick={() => onStartBridge(profile)}
                          >
                            {busy ? '处理中…' : recovery?.startLabel ?? (bridgeStatus?.state === 'error' ? '重试启动' : '启动')}
                          </Button>
                        )}
                      </>
                    )}
                    <Button variant="dangerOutline" size="sm" disabled={removing || busy} onClick={() => onRemove(profile)}>
                      {removing ? '删除中…' : '删除'}
                    </Button>
                    <a className="text-xs text-info hover:underline" href="#/connections">在 Connections 查看</a>
                  </div>
                  {recovery && (
                    <p className="mt-2 text-xs text-warning" role="status">{recovery.hint}</p>
                  )}
                  {rowError && (
                    <div className="mt-2 space-y-1" role="alert">
                      <AdapterErrorLines error={errors[profile.id]} fallback="适配操作失败" />
                      <p className="text-xs text-secondary">
                        {rowError.hint} 行内错误不会展示凭据；可重试当前操作或查看 Connections 中的来源连接状态。
                      </p>
                    </div>
                  )}
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    </TableShell>
  );
}
