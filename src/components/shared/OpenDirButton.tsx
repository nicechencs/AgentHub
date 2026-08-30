import { FolderOpen } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

/**
 * Open-in-file-manager control. Only two appearances:
 * - icon-only: toolbars next to other icon buttons
 * - labeled: folder icon + 「目录」 / "Folder", next to a path
 */
export function OpenDirButton({
  labeled = false,
  disabled,
  title,
  onClick,
  className,
}: {
  labeled?: boolean;
  disabled?: boolean;
  title: string;
  onClick: () => void;
  className?: string;
}) {
  const { t } = useI18n();
  if (labeled) {
    return (
      <Button
        type="button"
        size="sm"
        variant="ghost"
        className={cn('h-7 shrink-0 px-2', className)}
        disabled={disabled}
        title={title}
        aria-label={title}
        onClick={onClick}
      >
        <FolderOpen className="h-3 w-3" />
        {t('common.directory')}
      </Button>
    );
  }
  return (
    <Button
      type="button"
      size="icon"
      variant="ghost"
      className={cn('h-7 w-7 shrink-0', className)}
      disabled={disabled}
      title={title}
      aria-label={title}
      onClick={onClick}
    >
      <FolderOpen className="h-3.5 w-3.5" />
    </Button>
  );
}
