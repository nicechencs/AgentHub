import { useEffect, useId } from 'react';
import { PanelRightClose } from 'lucide-react';
import { SourcePreview } from '@/components/shared/SourcePreview';
import { CopyableFileName } from '@/components/shared/CopyableFileName';
import { pathTailLabel, previewHeaderParts } from '@/components/shared/file-name-label';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { Tip } from '@/components/ui/tooltip';
import { CHAT_FILE_PREVIEW_MAX_CHARS } from '@/lib/source-preview';
import { hasEscPriorityOverlay } from '@/lib/skills/preview-keys';
import { cn } from '@/lib/utils';
import { ChatExpandAffordance } from './ChatExpandAffordance';
import {
  sameEditPath,
  turnEditDiffText,
  type TurnEditFile,
} from './chat-edit-preview';

function fileName(path: string): string {
  const parts = path.trim().split(/[/\\]/).filter(Boolean);
  return parts[parts.length - 1] ?? path.trim();
}

export function ChatTurnEditList({
  files,
  selectedPath,
  onSelect,
}: {
  files: TurnEditFile[];
  selectedPath?: string;
  onSelect: (file: TurnEditFile) => void;
}) {
  const { t } = useI18n();
  if (files.length === 0) return null;
  return (
    <section
      className="mb-2 rounded-card border border-border bg-subtle px-3 py-2 text-meta"
      data-help="chat-turn-edits"
      aria-label={t('chat.preview.viewEdit')}
    >
      <p className="font-medium text-secondary">{t('chat.preview.viewEdit')}</p>
      <ul className="mt-1.5 space-y-0.5">
        {files.map((file) => {
          const selected = Boolean(selectedPath && sameEditPath(selectedPath, file.path));
          const statusLabel = file.status === 'live'
            ? t('chat.process.toolEdit')
            : t('chat.process.toolEditDone');
          return (
            <li key={file.path}>
              <button
                type="button"
                className={cn(
                  'group flex w-full min-w-0 items-center gap-1.5 rounded-btn px-1 py-0.5 text-left hover:bg-hover',
                  file.status === 'live' && 'agent-progress-running font-medium text-primary',
                  file.status === 'done' && 'text-secondary',
                  selected && 'bg-hover',
                )}
                aria-expanded={selected}
                aria-current={selected ? 'true' : undefined}
                onClick={() => onSelect(file)}
              >
                <ChatExpandAffordance
                  expanded={selected}
                  label={selected ? t('chat.runtime.collapseRow') : t('chat.runtime.expandRow')}
                />
                <span className="shrink-0 text-muted">{statusLabel}</span>
                <Tip label={file.path} className="min-w-0 flex-1 truncate font-mono">
                  {pathTailLabel(file.path)}
                </Tip>
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

export function ChatEditPreviewPanel({
  file,
  open,
  width,
  onClose,
  className,
}: {
  file: TurnEditFile;
  open: boolean;
  width?: number;
  onClose: () => void;
  className?: string;
}) {
  const { t } = useI18n();
  const titleId = useId();
  const header = previewHeaderParts(file.path);
  const name = header.fileName || fileName(file.path);
  const diff = turnEditDiffText(file) ?? '';

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (hasEscPriorityOverlay()) return;
      e.preventDefault();
      onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <aside
      className={cn(
        'flex h-full min-h-0 min-w-0 shrink-0 flex-col overflow-hidden rounded-card border border-border bg-panel shadow-xs',
        className,
      )}
      style={width != null ? { width } : undefined}
      aria-labelledby={titleId}
      data-chat-edit-preview
    >
      <header className="shrink-0 border-b border-border">
        <div className="flex h-10 items-center gap-1.5 overflow-x-auto px-3">
          <div className="min-w-0 flex-1 basis-16">
            <Tip
              label={header.fullPath || name || t('chat.preview.viewEdit')}
              className="inline-flex min-w-0 max-w-full items-baseline gap-2"
            >
              <h2
                id={titleId}
                className="max-w-[70%] shrink-0 truncate text-sm font-semibold leading-tight text-primary"
              >
                {name || t('chat.preview.viewEdit')}
              </h2>
              {header.directoryLabel ? (
                <span className="min-w-0 truncate text-meta text-muted">
                  {header.directoryLabel}
                </span>
              ) : (
                <span className="min-w-0 truncate text-meta text-muted">
                  {t('chat.preview.viewEdit')}
                </span>
              )}
            </Tip>
          </div>
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7 shrink-0"
            aria-label={t('chat.preview.collapse')}
            title={t('chat.preview.collapse')}
            onClick={onClose}
          >
            <PanelRightClose className="h-4 w-4" />
          </Button>
        </div>
      </header>

      <div className="min-h-0 min-w-0 flex-1 overflow-hidden p-0">
        {diff.trim() ? (
          <SourcePreview
            value={diff}
            fileName={`${name || 'edit'}.diff`}
            format="diff"
            readOnly
            pretty={false}
            compressBlankLines={false}
            density="document"
            maxChars={CHAT_FILE_PREVIEW_MAX_CHARS}
            className="h-full rounded-none"
          />
        ) : (
          <p className="px-4 py-6 text-sm text-muted">{t('chat.preview.emptyBody')}</p>
        )}
      </div>

      <footer className="flex shrink-0 items-center gap-2 border-t border-border px-3 py-1.5">
        <CopyableFileName path={file.path} className="min-w-0 flex-1" />
      </footer>
    </aside>
  );
}
