import type { ReactNode } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { Tip } from '@/components/ui/tooltip';

/** 表单行：左侧标签 + 短说明；细节用 descriptionTip 悬停展示 */
export function SettingsRow({
  label,
  description,
  descriptionTip,
  children,
}: {
  label: string;
  description?: string;
  descriptionTip?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 py-3">
      <div className="min-w-0">
        <p className="text-sm">{label}</p>
        {description &&
          (descriptionTip ? (
            <Tip className="mt-0.5 block text-xs text-muted" label={descriptionTip}>
              {description}
            </Tip>
          ) : (
            <p className="mt-0.5 text-xs text-muted">{description}</p>
          ))}
      </div>
      <div className="flex w-48 shrink-0 items-center justify-end gap-2">{children}</div>
    </div>
  );
}

export function SettingsSkeleton() {
  return (
    <div className="space-y-4">
      <Skeleton className="h-9 w-72" />
      <Card>
        <CardContent className="divide-y divide-border pt-1">
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className="flex items-center justify-between py-4">
              <div className="space-y-2">
                <Skeleton className="h-4 w-24" />
                <Skeleton className="h-3 w-40" />
              </div>
              <Skeleton className="h-8 w-40" />
            </div>
          ))}
        </CardContent>
      </Card>
    </div>
  );
}
