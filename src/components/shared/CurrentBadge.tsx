import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

/** Single "当前" badge chrome for active connection / provider / source rows. */
export function CurrentBadge({ className }: { className?: string }) {
  return (
    <Badge variant="accent" className={cn(className)}>
      当前
    </Badge>
  );
}
