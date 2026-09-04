import { ExternalLink } from 'lucide-react';
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
import { useToast } from '@/components/ui/toast';
import { Hint, Tip } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { SkillListingDto } from '@/lib/api/skill';
import { localizeSkillMarketDescription } from '@/lib/i18n/stored-copy';
import { openExternalLink } from '@/lib/open-external';
import { cn } from '@/lib/utils';
import { StorageKey } from '@/lib/ui-preferences';

type ColumnKey = 'name' | 'provider' | 'version' | 'actions';

const WIDTH_SPECS: ColumnWidthSpec<ColumnKey>[] = [
  { key: 'name', defaultWidth: 300, minWidth: 160 },
  { key: 'provider', defaultWidth: 120, minWidth: 88 },
  { key: 'version', defaultWidth: 100, minWidth: 72 },
  { key: 'actions', defaultWidth: 168, minWidth: 140 },
];

const COLUMN_KEYS: ColumnKey[] = ['name', 'provider', 'version', 'actions'];

const COLUMN_WIDTHS_STORAGE_KEY = StorageKey.skillsMarketColumnWidths;

/** Resolve market detail page URL (backend-provided, with client fallback). */
export function resolveMarketDetailUrl(item: SkillListingDto): string | null {
  const fromApi = item.detailUrl?.trim();
  if (fromApi) return fromApi;

  if (item.providerId === 'skills.sh') {
    const id = item.id.trim().replace(/^market:skills\.sh:/, '');
    if (id.includes('/')) return `https://skills.sh/${id.replace(/^\/+/, '')}`;
  }

  if (item.providerId === 'skillhub.cn') {
    let raw = item.id.trim().replace(/^market:skillhub\.cn:/, '').replace(/^skillhub:/, '');
    // strip trailing @version when version starts with digit
    const at = raw.lastIndexOf('@');
    if (at > 0) {
      const ver = raw.slice(at + 1);
      if (/^\d/.test(ver)) raw = raw.slice(0, at);
    }
    // Official SPA: /skills/:slug or /skills/:namespace/:slug — encode each segment, keep `/`
    if (raw) {
      const path = raw
        .replace(/^\/+/, '')
        .split('/')
        .filter(Boolean)
        .map((seg) => encodeURIComponent(seg))
        .join('/');
      if (path) return `https://skillhub.cn/skills/${path}`;
    }
  }

  return null;
}

function providerLabel(providerId: string): string {
  if (providerId === 'skills.sh') return 'skills.sh';
  if (providerId === 'skillhub.cn') return 'skillhub.cn';
  return providerId;
}

export function SkillMarketTable({
  items,
  installingId,
  onInstall,
}: {
  items: SkillListingDto[];
  installingId?: string | null;
  onInstall?: (item: SkillListingDto) => void;
}) {
  const { toast } = useToast();
  const { t } = useI18n();
  const { widths, onResizeStart, onResizeKeyDown, totalWidth } = useColumnWidths(
    WIDTH_SPECS,
    COLUMN_WIDTHS_STORAGE_KEY,
  );
  const columnLabels: Record<ColumnKey, string> = {
    name: t('skills.market.colName'),
    provider: t('skills.market.colProvider'),
    version: t('skills.market.colVersion'),
    actions: t('skills.market.colActions'),
  };

  const openDetail = (url: string) => {
    void (async () => {
      try {
        await openExternalLink(url);
      } catch (e) {
        toast({
          title: t('skills.toast.openDetailFailed'),
          description: e instanceof Error ? e.message : String(e),
          variant: 'danger',
        });
      }
    })();
  };

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
                {columnLabels[key]}
                <ColumnResizeHandle
                  columnKey={key}
                  label={columnLabels[key]}
                  onResizeStart={onResizeStart}
                    onResizeKeyDown={onResizeKeyDown}
                />
              </TableHead>
            ))}
          </TableHeaderRow>
        </TableHeader>
        <TableBody>
          {items.map((item) => {
            const busy = installingId === item.id;
            const detailUrl = resolveMarketDetailUrl(item);
            return (
              <TableRow key={item.id} className={cn(item.installed && 'opacity-60')}>
                <TableCell className="min-w-0">
                  {detailUrl ? (
                    <Hint label={item.name}>
                      <button
                        type="button"
                        className="group inline-flex max-w-full items-center gap-1 text-left font-medium text-accent underline-offset-2 hover:underline"
                        onClick={(e) => {
                          e.stopPropagation();
                          openDetail(detailUrl);
                        }}
                      >
                        <span className="truncate">{item.name}</span>
                        <ExternalLink className="h-3 w-3 shrink-0 opacity-70 group-hover:opacity-100" />
                      </button>
                    </Hint>
                  ) : (
                    <Tip className="truncate font-medium" label={item.name}>
                      {item.name}
                    </Tip>
                  )}
                  {item.description ? (
                    <Tip
                      className="mt-0.5 line-clamp-1 truncate text-xs text-secondary"
                      label={localizeSkillMarketDescription(item.description, t)}
                    >
                      {localizeSkillMarketDescription(item.description, t)}
                    </Tip>
                  ) : null}
                </TableCell>
                <TableCell className="truncate text-xs text-muted">
                  <Tip className="block truncate" label={item.providerId}>
                    {providerLabel(item.providerId)}
                  </Tip>
                </TableCell>
                <TableCell className="truncate font-mono text-xs text-muted">
                  {item.version ? `v${item.version}` : t('skills.market.versionUnknown')}
                </TableCell>
                <TableCell>
                  <div className="flex flex-wrap items-center justify-end gap-1.5">
                    {detailUrl ? (
                      <Button
                        size="sm"
                        variant="outline"
                        title={t('skills.market.openDetailHint')}
                        onClick={() => openDetail(detailUrl)}
                      >
                        <ExternalLink className="mr-1 h-3 w-3" />
                        {t('skills.market.openDetail')}
                      </Button>
                    ) : null}
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={item.installed || busy || !onInstall}
                      title={
                        item.installed
                          ? t('skills.market.installedHint')
                          : t('skills.market.installHint')
                      }
                      onClick={() => onInstall?.(item)}
                    >
                      {item.installed
                        ? t('skills.market.installed')
                        : busy
                          ? t('skills.market.installing')
                          : t('skills.market.install')}
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    </TableShell>
  );
}
