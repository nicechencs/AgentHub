import type { LucideIcon } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { EmptyState } from '@/components/shared/EmptyState';
import { Button } from '@/components/ui/button';
import { BRIDGES_PATH } from '@/lib/bridges-path';
import { useI18n } from '@/components/shared/LanguageProvider';

/** 路由区「开发中」占位：统一空态外观。 */
export function SoonPane({
  icon,
  title,
  description,
  showListLink = true,
}: {
  icon: LucideIcon;
  title: string;
  description: string;
  showListLink?: boolean;
}) {
  const { t } = useI18n();
  const navigate = useNavigate();
  return (
    <EmptyState
      icon={icon}
      title={title}
      description={description}
      action={
        showListLink ? (
          <Button
            size="sm"
            variant="outline"
            className="mt-2"
            onClick={() => navigate(BRIDGES_PATH)}
          >
            {t('routes.nav.goToList')}
          </Button>
        ) : undefined
      }
    />
  );
}
