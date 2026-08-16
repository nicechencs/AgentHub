import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
} from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Check,
  ChevronDown,
  SendHorizontal,
  Square,
} from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { Notice } from '@/components/shared/Notice';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Hint } from '@/components/ui/tooltip';
import { agentDisplayName } from '@/config/agents';
import type { AgentId, Conversation, Provider } from '@/lib/types';
import { cn } from '@/lib/utils';
import { extractModel } from './chat-format';
import {
  blockerCopy,
  blockerPrimaryTarget,
  type ChatAgentPickerRow,
  type ChatConnectionPickerView,
  type ChatSendBlocker,
} from './chat-model';

/** Composer 正文区：约 1 行起、最多 ~12 行；超出后内部滚动，工具条始终贴底。 */
const COMPOSER_MIN_PX = 56;
const COMPOSER_MAX_PX = 240;

export function ChatComposer({
  draft,
  setDraft,
  sending,
  active,
  providers,
  primaryAgent,
  agentPickerLabel,
  connectionView,
  switchingProvider,
  hiddenIds,
  pickerRows,
  blockers,
  connectionCaption,
  onSend,
  onCancel,
  onToggleAgent,
  onSwitchProvider,
  onOpenSettings,
  onPickWorkingDirectory,
  onFocusConversation,
}: {
  draft: string;
  setDraft: (v: string) => void;
  sending: boolean;
  active: Conversation;
  providers: Provider[];
  primaryAgent: AgentId | null;
  agentPickerLabel: string;
  connectionView: ChatConnectionPickerView;
  switchingProvider: boolean;
  hiddenIds: Set<AgentId>;
  pickerRows: ChatAgentPickerRow[];
  blockers: ChatSendBlocker[];
  connectionCaption: string | null;
  onSend: () => void;
  onCancel: () => void;
  onToggleAgent: (id: AgentId) => void;
  onSwitchProvider: (id: string) => void;
  onOpenSettings: () => void;
  onPickWorkingDirectory: () => void;
  onFocusConversation: (id: string) => void;
}) {
  const navigate = useNavigate();
  const firstBlocker = blockers[0] ?? null;
  const hiddenBlocked = firstBlocker?.kind === 'hiddenAgents' ||
    active.agentIds.some((id) => hiddenIds.has(id));
  const sendingElsewhere = blockers.some((b) => b.kind === 'sendingElsewhere');
  const canSend = Boolean(draft.trim()) && blockers.length === 0 && !sending;
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const syncTextareaHeight = useCallback(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = 'auto';
    const contentH = el.scrollHeight;
    const next = Math.min(Math.max(contentH, COMPOSER_MIN_PX), COMPOSER_MAX_PX);
    el.style.height = `${next}px`;
    el.style.overflowY = contentH > COMPOSER_MAX_PX ? 'auto' : 'hidden';
  }, []);

  useLayoutEffect(() => {
    syncTextareaHeight();
  }, [draft, syncTextareaHeight]);

  useEffect(() => {
    const onResize = () => syncTextareaHeight();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [syncTextareaHeight]);

  const textareaDisabled = sending || hiddenBlocked || sendingElsewhere;
  const sendHint = firstBlocker ? blockerCopy(firstBlocker).text : '发送';

  return (
    <>
      {firstBlocker && (
        <BlockerNotice
          blocker={firstBlocker}
          onGoAgents={() => navigate('/agents')}
          onGoConnections={() =>
            navigate(primaryAgent ? `/connections?agent=${primaryAgent}` : '/connections')
          }
          onOpenSettings={onOpenSettings}
          onPickWorkingDirectory={onPickWorkingDirectory}
          onFocusConversation={onFocusConversation}
          onCancel={onCancel}
        />
      )}
      <div className="rounded-composer border border-border bg-panel shadow-xs">
        <textarea
          ref={textareaRef}
          className={cn(
            'block w-full resize-none overflow-x-hidden break-words bg-transparent',
            'px-4 pb-2 pt-3 text-body leading-[1.45] outline-none placeholder:text-muted',
            'disabled:cursor-not-allowed disabled:opacity-60',
          )}
          style={{ minHeight: COMPOSER_MIN_PX, maxHeight: COMPOSER_MAX_PX }}
          placeholder="发送消息给 Agent…（Shift+Enter 换行）"
          rows={1}
          value={draft}
          disabled={textareaDisabled}
          onChange={(e) => setDraft(e.target.value)}
          onInput={syncTextareaHeight}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              if (canSend) onSend();
            }
          }}
          aria-label="消息输入"
        />
        <div className="flex shrink-0 items-center gap-1.5 border-t border-border/50 px-2 py-2">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                disabled={sending || sendingElsewhere}
                className="inline-flex h-7 max-w-36 items-center gap-1.5 rounded-btn border border-border bg-subtle px-2 text-meta text-secondary hover:bg-hover disabled:opacity-50"
              >
                {active.agentIds[0] && <AgentLogo agentId={active.agentIds[0]} size="sm" />}
                <span className="truncate">{agentPickerLabel}</span>
                <ChevronDown className="h-3 w-3 shrink-0 opacity-60" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-56">
              <DropdownMenuLabel>选择 Agent（可多选）</DropdownMenuLabel>
              <DropdownMenuSeparator />
              {pickerRows.map((row) => {
                const checked = active.agentIds.includes(row.id);
                return (
                  <DropdownMenuCheckboxItem
                    key={row.id}
                    checked={checked}
                    disabled={sending || sendingElsewhere || (!row.selectable && !checked)}
                    onCheckedChange={() => onToggleAgent(row.id)}
                  >
                    <span className="flex items-center gap-2">
                      <AgentLogo agentId={row.id} size="sm" />
                      {agentDisplayName(row.id)}
                      {row.reason === 'hidden' && (
                        <span className="text-meta text-muted">已隐藏</span>
                      )}
                      {row.reason === 'noAuth' && (
                        <span className="text-meta text-muted">未配置授权</span>
                      )}
                    </span>
                  </DropdownMenuCheckboxItem>
                );
              })}
              {pickerRows.length === 0 && (
                <div className="px-2 py-1.5 text-meta text-muted">尚未安装任何 Agent</div>
              )}
            </DropdownMenuContent>
          </DropdownMenu>

          <DropdownMenu>
            <Hint label={connectionCaption ?? undefined}>
              <DropdownMenuTrigger asChild>
                <button
                  type="button"
                  disabled={
                    !primaryAgent ||
                    sending ||
                    sendingElsewhere ||
                    switchingProvider ||
                    Boolean(primaryAgent && hiddenIds.has(primaryAgent))
                  }
                  className="inline-flex h-7 max-w-44 items-center gap-1 rounded-btn border border-border bg-subtle px-2 text-meta text-secondary hover:bg-hover disabled:opacity-50"
                  aria-label={connectionCaption ?? '切换连接'}
                >
                  <span className="min-w-0 truncate">
                    {connectionView.label}
                    {connectionView.subtitle ? (
                      <span className="text-muted"> · {connectionView.subtitle}</span>
                    ) : null}
                  </span>
                  <ChevronDown className="h-3 w-3 shrink-0 opacity-60" />
                </button>
              </DropdownMenuTrigger>
            </Hint>
            <DropdownMenuContent align="start" className="w-64">
              <DropdownMenuLabel>
                {primaryAgent ? `${agentDisplayName(primaryAgent)} · 切换连接` : '切换连接'}
              </DropdownMenuLabel>
              {connectionCaption && (
                <p className="px-2 pb-1.5 text-meta text-muted">{connectionCaption}</p>
              )}
              <DropdownMenuSeparator />
              {connectionView.currentLoginTitle && (
                <DropdownMenuItem disabled>
                  <span className="flex min-w-0 flex-1 items-center gap-2">
                    <Check className="h-3.5 w-3.5 shrink-0 text-accent" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate">{connectionView.currentLoginTitle}</span>
                      {connectionView.currentLoginSubtitle ? (
                        <span className="block truncate text-meta text-muted">
                          {connectionView.currentLoginSubtitle}
                        </span>
                      ) : null}
                    </span>
                  </span>
                </DropdownMenuItem>
              )}
              {providers.map((p) => {
                const model = extractModel(p.configText);
                const isCurrent = connectionView.kind === 'api' && p.isCurrent;
                return (
                  <DropdownMenuItem
                    key={p.id}
                    disabled={isCurrent || switchingProvider}
                    onClick={() => onSwitchProvider(p.id)}
                  >
                    <span className="flex min-w-0 flex-1 items-center gap-2">
                      {isCurrent ? (
                        <Check className="h-3.5 w-3.5 shrink-0 text-accent" />
                      ) : (
                        <span className="w-3.5 shrink-0" />
                      )}
                      <span className="min-w-0 flex-1">
                        <span className="block truncate">{p.name}</span>
                        {model ? (
                          <span className="block truncate text-meta text-muted">{model}</span>
                        ) : null}
                      </span>
                    </span>
                  </DropdownMenuItem>
                );
              })}
              {connectionView.emptyHint && providers.length === 0 && (
                <p className="px-2 py-1.5 text-meta text-muted">{connectionView.emptyHint}</p>
              )}
              {primaryAgent && (
                <div className="px-2 py-2">
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => navigate(`/connections?agent=${primaryAgent}`)}
                  >
                    {connectionView.manageLabel}
                  </Button>
                </div>
              )}
            </DropdownMenuContent>
          </DropdownMenu>

          <div className="flex-1" />

          {sending ? (
            <Button size="sm" variant="dangerOutline" onClick={onCancel}>
              <Square className="h-3.5 w-3.5" />
              停止
            </Button>
          ) : (
            <Button
              size="icon"
              variant={canSend ? 'default' : 'secondary'}
              className="h-7 w-7 rounded-btn"
              disabled={!canSend}
              onClick={onSend}
              title={sendHint}
            >
              <SendHorizontal className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
      </div>
      <p
        className={cn(
          'mt-2 text-center text-meta',
          active.allowDangerous ? 'text-warning' : 'text-muted',
        )}
      >
        {active.allowDangerous
          ? '自动批准已开启 · Agent 将不经确认修改文件'
          : 'Agent 可能修改工作目录中的文件'}
      </p>
    </>
  );
}

function BlockerNotice({
  blocker,
  onGoAgents,
  onGoConnections,
  onOpenSettings,
  onPickWorkingDirectory,
  onFocusConversation,
  onCancel,
}: {
  blocker: ChatSendBlocker;
  onGoAgents: () => void;
  onGoConnections: () => void;
  onOpenSettings: () => void;
  onPickWorkingDirectory: () => void;
  onFocusConversation: (id: string) => void;
  onCancel: () => void;
}) {
  const copy = blockerCopy(blocker);
  if (blocker.kind === 'sendingElsewhere') {
    return (
      <Notice tone="warning" className="mb-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span>{copy.text}</span>
          <span className="flex items-center gap-1">
            <Button
              size="sm"
              variant="outline"
              className="h-6 px-2 text-meta"
              onClick={() => onFocusConversation(blocker.conversationId)}
            >
              {copy.primaryAction}
            </Button>
            <Button
              size="sm"
              variant="dangerOutline"
              className="h-6 px-2 text-meta"
              onClick={onCancel}
            >
              {copy.secondaryAction}
            </Button>
          </span>
        </div>
      </Notice>
    );
  }
  return (
    <Notice
      tone="warning"
      className="mb-2"
      actionLabel={copy.primaryAction}
      onAction={
        {
          agents: onGoAgents,
          connections: onGoConnections,
          'pick-directory': onPickWorkingDirectory,
          settings: onOpenSettings,
        }[blockerPrimaryTarget(blocker)]
      }
    >
      {copy.text}
    </Notice>
  );
}
