import { useI18n } from '@/components/shared/LanguageProvider';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

/** Single "当前" badge chrome for active connection / provider / source rows. */
export function CurrentBadge({ className }: { className?: string }) {
  const { t } = useI18n();
  return (
    <Badge variant="accent" className={cn(className)}>
      {t('kind.current')}
    </Badge>
  );
}
