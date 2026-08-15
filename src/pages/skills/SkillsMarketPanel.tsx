import { Store } from 'lucide-react';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { SearchField } from '@/components/shared/SearchField';
import { TableSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import type { SkillListingDto } from '@/lib/api/skill';
import { openExternalLink } from '@/lib/open-external';
import type { SkillMarketSource } from '@/lib/types';
import { skillsCopy } from './copy';
import { SkillMarketTable } from './SkillMarketTable';
import { marketHomeUrl, marketResultLabel } from './skills-preview-model';

export type SkillsMarketPanelProps = {
  marketQuery: string;
  onMarketQueryChange: (v: string) => void;
  skillMarketSource: SkillMarketSource;
  activeMarketProvider: string | undefined;
  loading: boolean;
  error: unknown | null;
  onRetry: () => void;
  items: SkillListingDto[] | null | undefined;
  installingId: string | null;
  onInstall: (item: SkillListingDto) => void;
};

export function SkillsMarketPanel({
  marketQuery,
  onMarketQueryChange,
  skillMarketSource,
  activeMarketProvider,
  loading,
  error,
  onRetry,
  items,
  installingId,
  onInstall,
}: SkillsMarketPanelProps) {
  const { toast } = useToast();
  return (
    <>
      <div className="mb-3 flex flex-wrap items-center gap-3">
        <SearchField
          className="w-72"
          value={marketQuery}
          onChange={(e) => onMarketQueryChange(e.target.value)}
          placeholder={skillsCopy.filters.marketSearchPlaceholder}
        />
        <p className="text-xs text-muted">
          <button
            type="button"
            className="text-accent underline-offset-2 hover:underline"
            onClick={() => {
              const url = marketHomeUrl(activeMarketProvider, skillMarketSource);
              void openExternalLink(url).catch((e) => {
                toast({
                  title: '无法打开链接',
                  description: e instanceof Error ? e.message : String(e),
                  variant: 'danger',
                });
              });
            }}
          >
            {marketResultLabel(activeMarketProvider, skillMarketSource)}
          </button>
          {' · '}
          {skillsCopy.market.suffix(
            skillMarketSource === 'auto' &&
              (activeMarketProvider === 'skills.sh' ||
                activeMarketProvider === 'skillhub.cn'),
          )}
        </p>
      </div>
      {loading ? (
        <TableSkeleton rows={4} cols={3} />
      ) : error ? (
        <ErrorState error={error} onRetry={onRetry} />
      ) : !items?.length ? (
        <EmptyState
          icon={Store}
          title={skillsCopy.empty.marketNoneTitle}
          description={skillsCopy.empty.marketNoneDesc}
        />
      ) : (
        <SkillMarketTable
          items={items}
          installingId={installingId}
          onInstall={onInstall}
        />
      )}
    </>
  );
}
