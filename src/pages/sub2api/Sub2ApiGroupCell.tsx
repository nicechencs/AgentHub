import { ChevronDown } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Badge } from '@/components/ui/badge';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import type { Sub2ApiGroup } from '@/lib/sub2api';
import { formatGroupRate } from './sub2api-page-model';

function groupRateLabel(group: Sub2ApiGroup): string | null {
  const n = group.rate_multiplier;
  if (typeof n !== 'number' || !Number.isFinite(n)) return null;
  return formatGroupRate(n);
}

export function Sub2ApiGroupCell({
  label,
  rate,
  groupId,
  groups,
  disabled = false,
  onSelect,
}: {
  label: string | null;
  rate: string | null;
  groupId: number | null;
  groups: readonly Sub2ApiGroup[];
  disabled?: boolean;
  onSelect: (groupId: number | null) => void;
}) {
  const { t } = useI18n();
  const body = label ? (
    <div className="flex min-w-0 flex-wrap items-center gap-1">
      <Badge variant="accent">{label}</Badge>
      {rate ? <Badge variant="default">{rate}</Badge> : null}
    </div>
  ) : (
    <span className="text-xs text-secondary">{t('routes.sub2api.groupNone')}</span>
  );

  if (groups.length === 0) return body;

  return (
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className="inline-flex max-w-full items-center gap-1 rounded-btn px-1 py-0.5 text-left hover:bg-hover disabled:opacity-50"
          disabled={disabled}
          aria-label={t('routes.sub2api.selectGroup')}
          data-sub2api-group-edit=""
        >
          {body}
          <ChevronDown className="h-3 w-3 shrink-0 text-muted" aria-hidden />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="min-w-[16rem]">
        {groups.map((group) => {
          const rateLabel = groupRateLabel(group);
          return (
            <DropdownMenuItem
              key={group.id}
              disabled={group.id === groupId}
              className="justify-between gap-4"
              onSelect={() => onSelect(group.id)}
            >
              <span className="min-w-0 truncate">{group.name}</span>
              {rateLabel ? (
                <span className="ml-auto shrink-0 text-meta text-muted">{rateLabel}</span>
              ) : null}
            </DropdownMenuItem>
          );
        })}
        <DropdownMenuItem disabled={groupId == null} onSelect={() => onSelect(null)}>
          {t('routes.sub2api.groupNone')}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
