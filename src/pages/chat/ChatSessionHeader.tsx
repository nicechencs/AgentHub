import { useEffect, useRef, useState } from 'react';
import { FolderOpen, PanelLeftOpen, Settings2, ShieldAlert } from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Input } from '@/components/ui/input';
import { Hint } from '@/components/ui/tooltip';
import { agentDisplayName } from '@/config/agents';
import type { AgentId, Conversation } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  autoApproveActive,
  autoApproveEffect,
  autoApproveHint,
  conversationTitle,
  cwdShortName,
} from './chat-model';

export function ChatSessionHeader({
  active,
  railOpen,
  hiddenIds,
  onExpandRail,
  onRename,
  onOpenSettings,
  onPickWorkingDirectory,
}: {
  active: Conversation | null;
  railOpen: boolean;
  hiddenIds: Set<AgentId>;
  onExpandRail: () => void;
  onRename: (next: string) => Promise<boolean>;
  onOpenSettings: () => void;
  onPickWorkingDirectory: () => void;
}) {
  const { t } = useI18n();
  const [editing, setEditing] = useState(false);
  const [draftTitle, setDraftTitle] = useState(active?.title ?? '');
  const cancelledRef = useRef(false);
  const committedRef = useRef(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setEditing(false);
    setDraftTitle(active?.title ?? '');
  }, [active?.id]);

  useEffect(() => {
    if (editing) inputRef.current?.focus();
  }, [editing]);

  const hasHidden = Boolean(active?.agentIds.some((id) => hiddenIds.has(id)));
  const selectedAgent = active?.agentIds[0] ?? null;
  const approveOn = autoApproveActive(Boolean(active?.allowDangerous), selectedAgent);

  async function commit() {
    if (cancelledRef.current) {
      cancelledRef.current = false;
      return;
    }
    if (committedRef.current) return;
    committedRef.current = true;
    setEditing(false);
    const ok = await onRename(draftTitle);
    if (!ok) setDraftTitle(active?.title ?? '');
  }

  return (
    <header
      className={cn(
        'flex h-10 shrink-0 items-center gap-2 border-b border-border',
        pageRhythm.chatChromeX,
      )}
    >
      {!railOpen && (
        <Hint label={t('chat.rail.expandHistory')}>
          <button
            type="button"
            className="flex h-7 w-7 items-center justify-center rounded-btn text-muted hover:bg-hover hover:text-primary"
            onClick={onExpandRail}
            aria-label={t('chat.rail.expandHistory')}
          >
            <PanelLeftOpen className="h-4 w-4" />
          </button>
        </Hint>
      )}
      <div className="min-w-0 flex-1">
        {active && editing ? (
          <Input
            ref={inputRef}
            value={draftTitle}
            aria-label={t('chat.header.titleAria')}
            className="h-7 max-w-xs font-semibold"
            onChange={(e) => setDraftTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                void commit();
              } else if (e.key === 'Escape') {
                e.preventDefault();
                cancelledRef.current = true;
                setDraftTitle(active.title);
                setEditing(false);
              }
            }}
            onBlur={() => void commit()}
          />
        ) : (
          <button
            type="button"
            className="max-w-full truncate text-left text-body font-semibold text-primary"
            onClick={() => {
              if (!active) return;
              committedRef.current = false;
              cancelledRef.current = false;
              setDraftTitle(active.title);
              setEditing(true);
            }}
            disabled={!active}
          >
            {active ? conversationTitle(t, active.title) : t('chat.header.conversation')}
          </button>
        )}
      </div>
      {active && (
        <div className="flex min-w-0 shrink-0 items-center gap-1.5">
          <AgentChip agentIds={active.agentIds} hasHidden={hasHidden} />
          <Hint label={active.cwd || t('chat.header.pickCwd')}>
            <button
              type="button"
              onClick={onPickWorkingDirectory}
              className={cn(
                'inline-flex h-7 max-w-[9rem] items-center gap-1 rounded-btn border border-border bg-subtle px-2 text-meta',
                active.cwd ? 'text-secondary hover:bg-hover' : 'text-warning hover:bg-hover',
              )}
            >
              <FolderOpen className="h-3 w-3 shrink-0" />
              <span className="truncate">
                {active.cwd ? cwdShortName(active.cwd, t) : t('chat.header.cwdUnset')}
              </span>
            </button>
          </Hint>
          {approveOn && (
            <Hint label={autoApproveHint(t, autoApproveEffect(selectedAgent))}>
              <button
                type="button"
                onClick={onOpenSettings}
                className="inline-flex h-7 items-center gap-1 rounded-btn border border-border bg-subtle px-2 text-meta text-warning hover:bg-hover"
              >
                <ShieldAlert className="h-3 w-3 shrink-0" />
                {t('chat.header.autoApprove')}
              </button>
            </Hint>
          )}
          <Hint label={t('chat.header.sessionSettings')}>
            <button
              type="button"
              className="flex h-7 w-7 items-center justify-center rounded-btn text-muted hover:bg-hover hover:text-primary"
              aria-label={t('chat.header.sessionSettings')}
              onClick={onOpenSettings}
            >
              <Settings2 className="h-4 w-4" />
            </button>
          </Hint>
        </div>
      )}
    </header>
  );
}

function AgentChip({ agentIds, hasHidden }: { agentIds: AgentId[]; hasHidden: boolean }) {
  const { t } = useI18n();
  if (agentIds.length === 0) return null;
  if (agentIds.length === 1) {
    return (
      <span className="inline-flex h-7 items-center gap-1.5 rounded-btn border border-border bg-subtle px-2 text-meta text-secondary">
        <AgentLogo agentId={agentIds[0]} size="sm" />
        <span className="truncate">{agentDisplayName(agentIds[0])}</span>
        {hasHidden && <span className="text-muted">{t('chat.header.hidden')}</span>}
      </span>
    );
  }
  return (
    <span className="inline-flex h-7 items-center gap-1.5 rounded-btn border border-border bg-subtle px-2 text-meta text-secondary">
      <span className="flex items-center">
        {agentIds.slice(0, 3).map((id, i) => (
          <span key={id} className={cn(i > 0 && '-ml-1.5')} style={{ zIndex: 3 - i }}>
            <AgentLogo agentId={id} size="sm" />
          </span>
        ))}
      </span>
      <span>{t('chat.header.agentCount', { n: agentIds.length })}</span>
      {hasHidden && <span className="text-muted">{t('chat.header.hidden')}</span>}
    </span>
  );
}
