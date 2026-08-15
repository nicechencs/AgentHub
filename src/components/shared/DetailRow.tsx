import { cn } from '@/lib/utils';

/** Label + value pair used in expandable connection/account detail grids. */
export function DetailRow({
  label,
  value,
  mono,
  className,
}: {
  label: string;
  value: string;
  mono?: boolean;
  className?: string;
}) {
  return (
    <span className={cn('min-w-0', className)}>
      <span className="text-muted">{label} </span>
      {mono ? (
        <code className="break-all font-mono text-secondary">{value}</code>
      ) : (
        <span className="break-all text-secondary">{value}</span>
      )}
    </span>
  );
}
