import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
} from 'react';
import {
  Check,
  ChevronDown,
  SendHorizontal,
  Square,
} from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
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
import { Hint, Tip } from '@/components/ui/tooltip';
import { AGENT_IDS, agentDisplayName } from '@/config/agents';
import type { AgentId, Conversation, Provider } from '@/lib/types';
import { cn } from '@/lib/utils';
import { extractModel } from './chat-format';

/** Composer 正文区：约 1 行起、最多 ~12 行；超出后内部滚动，工具条始终贴底。 */
const COMPOSER_MIN_PX = 56;
const COMPOSER_MAX_PX = 240;

export function ChatComposer({
  draft,
  setDraft,
  sending,
  active,
  installed,
  providers,
  primaryAgent,
  agentPickerLabel,
  modelPickerLabel,
  modelPickerSubtitle,
  switchingProvider,
  onSend,
  onCancel,
  onToggleAgent,
  onSwitchProvider,
  hiddenIds,
}: {
  draft: string;
  setDraft: (v: string) => void;
  sending: boolean;
  active: Conversation;
  installed: Map<AgentId, boolean>;
  providers: Provider[];
  primaryAgent: AgentId | null;
  agentPickerLabel: string;
  modelPickerLabel: string;
  modelPickerSubtitle: string | null;
  switchingProvider: boolean;
  onSend: () => void;
  onCancel: () => void;
  onToggleAgent: (id: AgentId) => void;
  onSwitchProvider: (id: string) => void;
  hiddenIds: Set<AgentId>;
}) {
  const activeHasHidden = active.agentIds.some((id) => hiddenIds.has(id));
  const canSend = Boolean(draft.trim()) && !activeHasHidden;
  const pickerIds = AGENT_IDS.filter((id) => {
    if (installed.get(id) === false) return false;
    if (hiddenIds.has(id)) return active.agentIds.includes(id);
    return true;
  });
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const syncTextareaHeight = useCallback(() => {
    const el = textareaRef.current;
    if (!el) return;
    // 先收成 auto 再量 scrollHeight，避免删字后高度卡住
    el.style.height = 'auto';
    const contentH = el.scrollHeight;
    const next = Math.min(Math.max(contentH, COMPOSER_MIN_PX), COMPOSER_MAX_PX);
    el.style.height = `${next}px`;
    el.style.overflowY = contentH > COMPOSER_MAX_PX ? 'auto' : 'hidden';
  }, []);

  useLayoutEffect(() => {
    syncTextareaHeight();
  }, [draft, syncTextareaHeight]);

  // 窗口变窄换行后需重算高度
  useEffect(() => {
    const onResize = () => syncTextareaHeight();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [syncTextareaHeight]);

  return (
    <>
      <div className="rounded-composer border border-border bg-panel shadow-xs">
        <textarea
          ref={textareaRef}
          className={cn(
            'block w-full resize-none overflow-x-hidden break-words bg-transparent',
            'px-4 pb-2 pt-3 text-sm leading-[1.45] outline-none placeholder:text-muted',
            'disabled:cursor-not-allowed disabled:opacity-60',
          )}
          style={{ minHeight: COMPOSER_MIN_PX, maxHeight: COMPOSER_MAX_PX }}
          placeholder={
            activeHasHidden
              ? '当前会话包含已隐藏 Agent，请先取消隐藏'
              : '发送消息给 Agent…（Shift+Enter 换行）'
          }
          rows={1}
          value={draft}
          disabled={sending || activeHasHidden}
          onChange={(e) => setDraft(e.target.value)}
          onInput={syncTextareaHeight}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              onSend();
            }
          }}
          aria-label="消息输入"
        />
        <div className="flex shrink-0 items-center gap-1.5 border-t border-border/50 px-2 py-2">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                disabled={sending}
                className="inline-flex h-7 max-w-36 items-center gap-1.5 rounded-btn border border-border bg-subtle px-2 text-xs text-secondary hover:bg-hover disabled:opacity-50"
              >
                {active.agentIds[0] && <AgentLogo agentId={active.agentIds[0]} size="sm" />}
                <span className="truncate">{agentPickerLabel}</span>
                <ChevronDown className="h-3 w-3 shrink-0 opacity-60" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-56">
              <DropdownMenuLabel>选择 Agent（可多选）</DropdownMenuLabel>
              <DropdownMenuSeparator />
              {pickerIds.map((id) => (
                <DropdownMenuCheckboxItem
                  key={id}
                  checked={active.agentIds.includes(id)}
                  disabled={sending || (hiddenIds.has(id) && !active.agentIds.includes(id))}
                  onCheckedChange={() => onToggleAgent(id)}
                >
                  <span className="flex items-center gap-2">
                    <AgentLogo agentId={id} size="sm" />
                    {agentDisplayName(id)}
                    {hiddenIds.has(id) && (
                      <span className="text-xs text-muted">已隐藏</span>
                    )}
                  </span>
                </DropdownMenuCheckboxItem>
              ))}
              {pickerIds.length === 0 && (
                <div className="px-2 py-1.5 text-xs text-muted">尚未安装任何 Agent</div>
              )}
            </DropdownMenuContent>
          </DropdownMenu>

          <DropdownMenu>
            <Hint
              label={
                active.agentIds.length > 1 && primaryAgent
                  ? `连接切换作用于首个 Agent（${agentDisplayName(primaryAgent)}）`
                  : undefined
              }
            >
              <DropdownMenuTrigger asChild>
                <button
                  type="button"
                  disabled={
                    !primaryAgent ||
                    sending ||
                    switchingProvider ||
                    Boolean(primaryAgent && hiddenIds.has(primaryAgent))
                  }
                  className="inline-flex h-7 max-w-44 items-center gap-1 rounded-btn border border-border bg-subtle px-2 text-xs text-secondary hover:bg-hover disabled:opacity-50"
                  aria-label={
                    active.agentIds.length > 1
                      ? `连接切换作用于首个 Agent（${agentDisplayName(primaryAgent!)}）`
                      : '切换连接'
                  }
                >
                  <span className="min-w-0 truncate">
                    {modelPickerLabel}
                    {modelPickerSubtitle ? (
                      <span className="text-muted"> · {modelPickerSubtitle}</span>
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
              <DropdownMenuSeparator />
              {providers.length === 0 ? (
                <div className="px-2 py-3 text-xs text-muted">暂无连接，去连接页添加</div>
              ) : (
                providers.map((p) => {
                  const model = extractModel(p.configText);
                  return (
                    <DropdownMenuItem
                      key={p.id}
                      disabled={p.isCurrent || switchingProvider}
                      onClick={() => onSwitchProvider(p.id)}
                    >
                      <span className="flex min-w-0 flex-1 items-center gap-2">
                        {p.isCurrent ? (
                          <Check className="h-3.5 w-3.5 shrink-0 text-accent" />
                        ) : (
                          <span className="w-3.5 shrink-0" />
                        )}
                        <span className="min-w-0 flex-1">
                          <span className="block truncate">{p.name}</span>
                          {model ? (
                            <span className="block truncate text-xs text-muted">{model}</span>
                          ) : null}
                        </span>
                      </span>
                    </DropdownMenuItem>
                  );
                })
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
              title="发送"
            >
              <SendHorizontal className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
      </div>
      <div className="mt-2 flex justify-center px-2">
        <Tip
          className="inline-flex max-w-[min(100%,28rem)] items-center gap-1 text-xs text-muted"
          label={[
            'Agent 可能改动工作目录中的文件',
            active.cwd ?? '工作目录未设置',
            active.allowDangerous
              ? '自动批准已开启：跳过工具确认'
              : '自动批准已关闭：工具调用需确认',
          ].join(' · ')}
        >
          <span className="min-w-0 truncate">
            {active.cwd ?? '未设置工作目录'}
          </span>
          <span className="shrink-0">
            · {active.allowDangerous ? '自动批准' : '需确认'}
          </span>
        </Tip>
      </div>
    </>
  );
}
