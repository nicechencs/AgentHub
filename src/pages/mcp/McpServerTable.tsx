import { useId, useState } from 'react';
import { AgentDot } from '@/components/shared/AgentDot';
import { DetailsToggle } from '@/components/shared/DetailsToggle';
import { CopyableFileName } from '@/components/shared/CopyableFileName';
import { OpenDirButton } from '@/components/shared/OpenDirButton';
import { SourcePreview } from '@/components/shared/SourcePreview';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Badge } from '@/components/ui/badge';
import {
  ColumnResizeHandle,
  Table,
  TableBody,
  TableCell,
  TableEmptyCell,
  TableHead,
  TableHeader,
  TableHeaderRow,
  TableRow,
  TableShell,
  useColumnWidths,
  type ColumnWidthSpec,
} from '@/components/ui/table';
import { Tip } from '@/components/ui/tooltip';
import { agentDisplayName } from '@/config/agents';
import type { McpServerEntry } from '@/lib/backend/contracts/mcp-types';
import type { AgentKey } from '@/lib/types';
import type { TranslateFn } from '@/lib/i18n';
import { StorageKey } from '@/lib/ui-preferences';
import type { McpAgentGroup } from './group-servers';

type ColumnKey = 'name' | 'transport' | 'endpoint' | 'actions';

const WIDTH_SPECS: ColumnWidthSpec<ColumnKey>[] = [
  { key: 'name', defaultWidth: 200, minWidth: 120 },
  { key: 'transport', defaultWidth: 88, minWidth: 64 },
  { key: 'endpoint', defaultWidth: 360, minWidth: 160 },
  { key: 'actions', defaultWidth: 120, minWidth: 96 },
];

const COLUMN_KEYS: ColumnKey[] = ['name', 'transport', 'endpoint', 'actions'];

const COLUMN_WIDTHS_STORAGE_KEY = StorageKey.mcpColumnWidths;

function columnLabels(t: TranslateFn): Record<ColumnKey, string> {
  return {
    name: t('mcp.table.name'),
    transport: t('mcp.table.transport'),
    endpoint: t('mcp.table.endpoint'),
    actions: t('mcp.table.actions'),
  };
}

function transportLabel(transport: string, t: TranslateFn): string {
  switch (transport) {
    case 'stdio':
      return 'stdio';
    case 'sse':
      return 'SSE';
    case 'http':
      return 'HTTP';
    default:
      return transport || t('mcp.table.unknown');
  }
}

function displayText(value?: string | null): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

function endpointOf(server: McpServerEntry): string | null {
  return displayText(server.command) ?? displayText(server.url);
}

function FileGroupHeader({
  agent,
  showAgent,
  path,
  colSpan,
  onLocate,
}: {
  agent: AgentKey;
  showAgent: boolean;
  path: string;
  colSpan: number;
  onLocate: (path: string) => void;
}) {
  return (
    <TableRow className="bg-subtle hover:bg-subtle">
      <TableCell colSpan={colSpan} className="py-1.5">
        <div className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2">
            {showAgent ? (
              <>
                <AgentDot agentId={agent} className="h-2 w-2" />
                <span className="shrink-0 text-body font-medium">{agentDisplayName(agent)}</span>
                <span className="text-muted">·</span>
              </>
            ) : null}
            <CopyableFileName path={path} className="min-w-0" />
          </div>
          <OpenDirButton
            labeled
            title={path}
            onClick={() => void onLocate(path)}
          />
        </div>
      </TableCell>
    </TableRow>
  );
}

function ServerTableRow({ server }: { server: McpServerEntry }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const detailsId = useId();
  const endpoint = endpointOf(server);
  const hasSnippet = Boolean(server.snippet?.trim());
  return (
    <>
      <TableRow>
        <TableCell className="min-w-0">
          <div className="flex min-w-0 items-center gap-2">
            <span className="truncate font-medium">{server.name}</span>
            {server.enabled === false ? <Badge variant="warning">{t('mcp.table.disabled')}</Badge> : null}
          </div>
        </TableCell>
        <TableCell className="truncate text-meta text-secondary">
          {transportLabel(server.transport, t)}
        </TableCell>
        <TableCell className="min-w-0">
          {endpoint ? (
            <Tip className="block truncate font-mono text-meta text-secondary" label={endpoint}>
              {endpoint}
            </Tip>
          ) : (
            <TableEmptyCell />
          )}
        </TableCell>
        <TableCell>
          {hasSnippet ? (
            <DetailsToggle open={open} controlsId={detailsId} onClick={() => setOpen((v) => !v)}>
              {t('mcp.table.details')}
            </DetailsToggle>
          ) : (
            <TableEmptyCell />
          )}
        </TableCell>
      </TableRow>
      {open && hasSnippet ? (
        <TableRow className="hover:bg-transparent">
          <TableCell colSpan={4} className="bg-subtle/50">
            <SourcePreview
              id={detailsId}
              value={server.snippet ?? ''}
              format={server.sourceFormat}
              showCopy
            />
          </TableCell>
        </TableRow>
      ) : null}
    </>
  );
}

export function McpServerTable({
  groups,
  showAgent,
  onLocate,
}: {
  groups: McpAgentGroup[];
  showAgent: boolean;
  onLocate: (path: string) => void;
}) {
  const { t } = useI18n();
  const labels = columnLabels(t);
  const { widths, onResizeStart, totalWidth } = useColumnWidths(
    WIDTH_SPECS,
    COLUMN_WIDTHS_STORAGE_KEY,
  );
  return (
    <TableShell>
      <Table className="table-fixed" style={{ minWidth: totalWidth }}>
        <colgroup>
          {WIDTH_SPECS.map((c) => (
            <col key={c.key} style={{ width: widths[c.key] }} />
          ))}
        </colgroup>
        <TableHeader>
          <TableHeaderRow>
            {COLUMN_KEYS.map((key) => (
              <TableHead key={key} className="relative select-none">
                {labels[key]}
                <ColumnResizeHandle
                  columnKey={key}
                  label={labels[key]}
                  onResizeStart={onResizeStart}
                />
              </TableHead>
            ))}
          </TableHeaderRow>
        </TableHeader>
        <TableBody>
          {groups.map((g) =>
            g.files.map((file) => (
              <FragmentGroup
                key={`${g.agent}:${file.sourcePath}`}
                agent={g.agent}
                showAgent={showAgent}
                path={file.sourcePath}
                servers={file.servers}
                onLocate={onLocate}
              />
            )),
          )}
        </TableBody>
      </Table>
    </TableShell>
  );
}

function FragmentGroup({
  agent,
  showAgent,
  path,
  servers,
  onLocate,
}: {
  agent: AgentKey;
  showAgent: boolean;
  path: string;
  servers: McpServerEntry[];
  onLocate: (path: string) => void;
}) {
  return (
    <>
      <FileGroupHeader
        agent={agent}
        showAgent={showAgent}
        path={path}
        colSpan={4}
        onLocate={onLocate}
      />
      {servers.map((s) => (
        <ServerTableRow key={`${s.agent}:${s.name}:${s.sourcePath}`} server={s} />
      ))}
    </>
  );
}
