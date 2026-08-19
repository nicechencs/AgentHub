import { useId, useState } from 'react';
import { ChevronDown, FolderOpen } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  ColumnResizeHandle,
  Table,
  TableBody,
  TableCell,
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
import type { AgentId } from '@/lib/types';
import { cn } from '@/lib/utils';
import type { TranslateFn } from '@/lib/i18n';
import type { McpAgentGroup } from './group-servers';

type ColumnKey = 'name' | 'transport' | 'endpoint' | 'actions';

const WIDTH_SPECS: ColumnWidthSpec<ColumnKey>[] = [
  { key: 'name', defaultWidth: 200, minWidth: 120 },
  { key: 'transport', defaultWidth: 88, minWidth: 64 },
  { key: 'endpoint', defaultWidth: 360, minWidth: 160 },
  { key: 'actions', defaultWidth: 96, minWidth: 80 },
];

const COLUMN_KEYS: ColumnKey[] = ['name', 'transport', 'endpoint', 'actions'];

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

function McpDetailsButton({
  open,
  detailsId,
  onToggle,
}: {
  open: boolean;
  detailsId: string;
  onToggle: () => void;
}) {
  const { t } = useI18n();
  return (
    <Button
      size="sm"
      variant="ghost"
      className="h-6 px-1.5 text-xs"
      aria-expanded={open}
      aria-controls={detailsId}
      onClick={onToggle}
    >
      {t('mcp.table.details')}
      <ChevronDown className={cn('h-3.5 w-3.5 transition-transform', open && 'rotate-180')} />
    </Button>
  );
}

function FileGroupHeader({
  agent,
  showAgent,
  path,
  colSpan,
  onLocate,
}: {
  agent: AgentId;
  showAgent: boolean;
  path: string;
  colSpan: number;
  onLocate: (path: string) => void;
}) {
  const { t } = useI18n();
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
            <Tip label={path}>
              <p className="min-w-0 truncate font-mono text-meta text-muted">{path}</p>
            </Tip>
          </div>
          <Button
            size="sm"
            variant="ghost"
            className="h-6 shrink-0 gap-1 px-1.5 text-xs"
            onClick={() => void onLocate(path)}
          >
            <FolderOpen className="h-3 w-3" />
            {t('mcp.table.directory')}
          </Button>
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
            <span className="text-muted">—</span>
          )}
        </TableCell>
        <TableCell>
          {hasSnippet ? (
            <McpDetailsButton
              open={open}
              detailsId={detailsId}
              onToggle={() => setOpen((v) => !v)}
            />
          ) : (
            <span className="text-muted">—</span>
          )}
        </TableCell>
      </TableRow>
      {open && hasSnippet ? (
        <TableRow className="hover:bg-transparent">
          <TableCell colSpan={4} className="bg-subtle/50">
            <pre
              id={detailsId}
              className="max-h-64 overflow-auto whitespace-pre-wrap break-words font-mono text-meta leading-relaxed text-secondary"
            >
              {server.snippet}
            </pre>
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
  const { widths, onResizeStart, totalWidth } = useColumnWidths(WIDTH_SPECS);
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
  agent: AgentId;
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
