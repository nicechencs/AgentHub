import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useSearchParams } from 'react-router-dom';
import {
  Check,
  ChevronDown,
  FolderOpen,
  Loader2,
  MessagesSquare,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  SendHorizontal,
  Settings2,
  Square,
  Terminal,
  Trash2,
} from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { MarkdownView } from '@/components/shared/MarkdownView';
import { Button } from '@/components/ui/button';
import { ListSkeleton, Skeleton } from '@/components/ui/skeleton';
import { Hint, Tip } from '@/components/ui/tooltip';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { useToast } from '@/components/ui/toast';
import { AGENT_IDS, AGENT_MAP, agentDisplayName } from '@/config/agents';
import { listAgents } from '@/lib/api/agent';
import {
  chatCancel,
  chatSend,
  createConversation,
  deleteConversation,
  listChatMessages,
  listConversations,
  updateConversation,
} from '@/lib/api/chat';
import { listProviders, switchProvider } from '@/lib/api/provider';
import { takeChatBootstrap } from '@/lib/chat-bootstrap';
import {
  hasProcessDetails,
  phaseFromMessageStatus,
  processKey,
  processPhaseLabel,
  reduceProcessEvent,
  stepSummary,
  type AgentProcessView,
  type ProcessMap,
} from '@/lib/chat-process';
import type {
  AgentId,
  AgentStatus,
  ChatEvent,
  ChatMessage,
  Conversation,
  ProcessStep,
  Provider,
} from '@/lib/types';
import { cn } from '@/lib/utils';

type TurnGroup = {
  turn: number;
  user?: ChatMessage;
  agents: ChatMessage[];
};

function formatStepInput(input: unknown): string | null {
  if (input == null) return null;
  try {
    const s = typeof input === 'string' ? input : JSON.stringify(input);
    return s.length > 240 ? `${s.slice(0, 240)}…` : s;
  } catch {
    return String(input);
  }
}

/** Render tool/stderr text; highlight unified-diff style lines when present. */
function DiffAwarePre({ text, className }: { text: string; className?: string }) {
  const looksDiff =
    /^(?:diff --git|@@ |--- |\+\+\+ )/m.test(text) ||
    (text.includes('\n+') && text.includes('\n-') && /^(?:[+-](?![+-])).+/m.test(text));

  if (!looksDiff) {
    return (
      <pre className={className}>{text.length > 4000 ? `${text.slice(0, 4000)}…` : text}</pre>
    );
  }

  const lines = text.split('\n').slice(0, 200);
  return (
    <pre className={cn(className, 'space-y-0')}>
      {lines.map((line, i) => {
        const tone =
          line.startsWith('+') && !line.startsWith('+++')
            ? 'text-success'
            : line.startsWith('-') && !line.startsWith('---')
              ? 'text-danger'
              : line.startsWith('@@')
                ? 'text-info'
                : 'text-secondary';
        return (
          <div key={i} className={cn('whitespace-pre-wrap break-all', tone)}>
            {line || ' '}
          </div>
        );
      })}
      {text.split('\n').length > 200 ? (
        <div className="text-muted">…已截断</div>
      ) : null}
    </pre>
  );
}

function formatDurationMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const m = Math.floor(ms / 60_000);
  const s = Math.round((ms % 60_000) / 1000);
  return `${m}m ${s}s`;
}

function ProcessStepRow({ step }: { step: ProcessStep }) {
  if (step.type === 'tool') {
    const input = formatStepInput(step.input);
    return (
      <div className="rounded-btn border border-border/80 bg-panel px-2 py-1.5">
        <div className="flex items-center gap-1.5 font-medium text-secondary">
          <span className="rounded-btn bg-subtle px-1 py-0.5 text-2xs uppercase tracking-wide text-muted">
            tool
          </span>
          <span>{step.name}</span>
          <span className="text-muted">· {step.status}</span>
        </div>
        {input ? (
          <pre className="mt-1 max-h-16 overflow-auto whitespace-pre-wrap break-all font-mono text-2xs text-muted">
            {input}
          </pre>
        ) : null}
        {step.result ? (
          <DiffAwarePre
            text={step.result}
            className="mt-1 max-h-28 overflow-auto rounded-btn bg-subtle/50 px-1.5 py-1 font-mono text-2xs leading-relaxed text-secondary"
          />
        ) : null}
      </div>
    );
  }
  if (step.type === 'thinking') {
    return (
      <div className="rounded-btn border border-dashed border-border/80 px-2 py-1.5 text-muted">
        <span className="mr-1.5 text-2xs uppercase tracking-wide">thinking</span>
        <span className="whitespace-pre-wrap">{step.text}</span>
      </div>
    );
  }
  if (step.type === 'error') {
    return <div className="text-danger">{step.message}</div>;
  }
  return (
    <div className="text-muted">
      <span className="mr-1 font-medium text-secondary">{step.type}</span>
      {stepSummary(step)}
    </div>
  );
}

function isProcessActivePhase(phase: AgentProcessView['phase']): boolean {
  return phase === 'queued' || phase === 'starting' || phase === 'running';
}

function isProcessErrorPhase(phase: AgentProcessView['phase']): boolean {
  return phase === 'failed' || phase === 'timeout';
}

/**
 * 过程面板（受控 open）：
 * - 进行中 / 失败 / 超时 → 默认展开
 * - 成功 / 取消 → 默认折叠
 * - messageStatus 优先于 process.phase（防止过程机滞后仍停在 running）
 * - 用户点击后记住选择；阶段变化时重新交给自动策略
 */
function AgentProcessPanel({
  view,
  messageStatus,
  durationMs,
  exitCode,
}: {
  view: AgentProcessView;
  /** 对应气泡消息状态；终态时强制驱动折叠策略 */
  messageStatus?: string;
  durationMs?: number;
  exitCode?: number | null;
}) {
  const timeline = view.steps.filter((s) => s.type !== 'text');
  const toolCount = timeline.filter((s) => s.type === 'tool').length;
  const thinkingCount = timeline.filter((s) => s.type === 'thinking').length;

  const effectivePhase: AgentProcessView['phase'] =
    messageStatus && messageStatus !== 'running'
      ? phaseFromMessageStatus(messageStatus)
      : view.phase;

  const autoOpen =
    isProcessActivePhase(effectivePhase) || isProcessErrorPhase(effectivePhase);

  const [userOpen, setUserOpen] = useState<boolean | null>(null);
  const phaseKeyRef = useRef(effectivePhase);

  // 阶段变化（含消息终态到位）时清掉手动覆盖，确保「结束后折叠」生效
  useLayoutEffect(() => {
    if (phaseKeyRef.current !== effectivePhase) {
      phaseKeyRef.current = effectivePhase;
      setUserOpen(null);
    }
  }, [effectivePhase]);

  const open = userOpen ?? autoOpen;

  return (
    <details
      className="mb-2 rounded-card border border-border bg-subtle/60 text-xs text-secondary"
      open={open}
      onToggle={(e) => {
        const next = e.currentTarget.open;
        // 与受控 open 对齐：只在用户意图与当前受控值不同时写入
        if (next !== open) {
          setUserOpen(next);
        }
      }}
    >
      <summary className="flex cursor-pointer list-none items-center gap-1.5 px-2.5 py-1.5 text-muted marker:content-none [&::-webkit-details-marker]:hidden">
        <Terminal className="h-3 w-3 shrink-0 opacity-70" />
        <span className="font-medium text-secondary">过程</span>
        <span className="text-muted">·</span>
        <span>{processPhaseLabel(effectivePhase)}</span>
        {timeline.length > 0 ? (
          <span className="text-muted">· {timeline.length} 步</span>
        ) : null}
        {durationMs != null && durationMs > 0 ? (
          <span className="text-muted">· {formatDurationMs(durationMs)}</span>
        ) : null}
        {view.command ? (
          <Tip
            className="ml-auto max-w-[45%] truncate font-mono text-2xs text-muted"
            label={view.command}
          >
            {view.command}
          </Tip>
        ) : null}
      </summary>
      <div className="space-y-2 border-t border-border px-2.5 py-2">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
          <span className="text-muted">状态</span>
          <span className="font-medium text-secondary">
            {processPhaseLabel(effectivePhase)}
          </span>
          {durationMs != null && durationMs > 0 ? (
            <>
              <span className="text-muted">·</span>
              <span className="text-muted">耗时 {formatDurationMs(durationMs)}</span>
            </>
          ) : null}
          {exitCode != null ? (
            <>
              <span className="text-muted">·</span>
              <span className="text-muted">exit {exitCode}</span>
            </>
          ) : null}
          {toolCount > 0 || thinkingCount > 0 ? (
            <>
              <span className="text-muted">·</span>
              <span className="text-muted">
                {[toolCount > 0 ? `工具 ${toolCount}` : null, thinkingCount > 0 ? `思考 ${thinkingCount}` : null]
                  .filter(Boolean)
                  .join(' · ')}
              </span>
            </>
          ) : null}
        </div>
        {view.command ? (
          <div>
            <div className="mb-0.5 text-muted">命令</div>
            <pre className="max-h-24 overflow-auto whitespace-pre-wrap break-all rounded-btn bg-panel px-2 py-1.5 font-mono text-2xs leading-relaxed text-primary">
              {view.command}
            </pre>
          </div>
        ) : null}
        {timeline.length > 0 ? (
          <div>
            <div className="mb-1 text-muted">步骤</div>
            <div className="max-h-48 space-y-1.5 overflow-y-auto">
              {timeline.map((step, i) => (
                <ProcessStepRow key={`${step.type}-${i}`} step={step} />
              ))}
            </div>
          </div>
        ) : null}
        {view.stderr ? (
          <div>
            <div className="mb-0.5 text-muted">stderr</div>
            <pre className="max-h-36 overflow-auto whitespace-pre-wrap break-all rounded-btn bg-panel px-2 py-1.5 font-mono text-2xs leading-relaxed text-danger/90">
              {view.stderr}
            </pre>
          </div>
        ) : !timeline.length && isProcessActivePhase(effectivePhase) ? (
          <p className="text-muted">等待 CLI 输出过程日志…</p>
        ) : null}
      </div>
    </details>
  );
}

function groupByTurn(messages: ChatMessage[]): TurnGroup[] {
  const map = new Map<number, TurnGroup>();
  for (const m of messages) {
    let g = map.get(m.turn);
    if (!g) {
      g = { turn: m.turn, agents: [] };
      map.set(m.turn, g);
    }
    if (m.role === 'user') g.user = m;
    else g.agents.push(m);
  }
  return [...map.values()].sort((a, b) => a.turn - b.turn);
}

/** 从 provider 配置文本里尽量抽出 model 名 */
function extractModel(configText: string): string | null {
  const toml = configText.match(/(?:^|\n)\s*(?:model|default_model)\s*=\s*"([^"]+)"/m);
  if (toml?.[1]) return toml[1];
  const json = configText.match(/"(?:model|default_model)"\s*:\s*"([^"]+)"/);
  return json?.[1] ?? null;
}

function relativeTime(iso: string): string {
  const t = Date.parse(iso.includes('T') ? iso : iso.replace(' ', 'T') + 'Z');
  if (Number.isNaN(t)) return '';
  const diff = Date.now() - t;
  const m = Math.floor(diff / 60000);
  if (m < 1) return '刚刚';
  if (m < 60) return `${m} 分钟前`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} 小时前`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d} 天前`;
  return new Date(t).toLocaleDateString();
}

/** Composer 正文区：约 1 行起、最多 ~12 行；超出后内部滚动，工具条始终贴底。 */
const COMPOSER_MIN_PX = 56;
const COMPOSER_MAX_PX = 240;

function ChatComposer({
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
}) {
  const canSend = Boolean(draft.trim());
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
          placeholder="发送消息给 Agent…（Shift+Enter 换行）"
          rows={1}
          value={draft}
          disabled={sending}
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
              {AGENT_IDS.filter((id) => installed.get(id) !== false).map((id) => (
                <DropdownMenuCheckboxItem
                  key={id}
                  checked={active.agentIds.includes(id)}
                  disabled={sending}
                  onCheckedChange={() => onToggleAgent(id)}
                >
                  <span className="flex items-center gap-2">
                    <AgentLogo agentId={id} size="sm" />
                    {agentDisplayName(id)}
                  </span>
                </DropdownMenuCheckboxItem>
              ))}
              {AGENT_IDS.every((id) => installed.get(id) === false) && (
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
                  disabled={!primaryAgent || sending || switchingProvider}
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

export default function ChatPage() {
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [agentStatus, setAgentStatus] = useState<AgentStatus[]>([]);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [error, setError] = useState<unknown>(null);
  /** 会话列表骨架（不再用整页 spinner 挡住消息区） */
  const [listLoading, setListLoading] = useState(true);
  /** 当前会话消息 / provider 加载 */
  const [messagesLoading, setMessagesLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [sendingConversationId, setSendingConversationId] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [railOpen, setRailOpen] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [dangerConfirm, setDangerConfirm] = useState(false);
  const [switchingProvider, setSwitchingProvider] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);
  const streamingRef = useRef<Record<string, string>>({});
  const activeIdRef = useRef<string | null>(null);
  /** 当轮过程面板：命令 / stderr / 细状态（仅内存，不落库） */
  const [processMap, setProcessMap] = useState<ProcessMap>({});
  /** Projects 页跳转：bootstrap 只处理一次 */
  const bootstrapDoneRef = useRef(false);
  activeIdRef.current = activeId;

  const active = useMemo(
    () => conversations.find((c) => c.id === activeId) ?? null,
    [conversations, activeId],
  );

  const installed = useMemo(() => {
    const m = new Map<AgentId, boolean>();
    for (const a of agentStatus) m.set(a.agentId, a.installed);
    return m;
  }, [agentStatus]);

  const primaryAgent = active?.agentIds[0] ?? null;

  const currentProvider = useMemo(
    () => providers.find((p) => p.isCurrent) ?? providers[0] ?? null,
    [providers],
  );

  const defaultAgents = useCallback(
    (agents: AgentStatus[]): AgentId[] => {
      const installedIds = agents.filter((a) => a.installed).map((a) => a.agentId);
      if (installedIds.length > 0) return [installedIds[0]];
      return ['claude'];
    },
    [],
  );

  /** 确保至少有一个会话；空列表时自动新建并返回完整列表。 */
  const ensureConversation = useCallback(
    async (convs: Conversation[], agents: AgentStatus[], cwd?: string | null) => {
      if (convs.length > 0) return convs;
      const created = await createConversation(defaultAgents(agents), cwd ?? null);
      return [created];
    },
    [defaultAgents],
  );

  /**
   * 会话列表优先：不因 listAgents（doctor）阻塞会话渲染。
   * agents 仅在空列表需自动建会话时才 await。
   */
  const loadList = useCallback(async () => {
    const convs = await listConversations();
    if (convs.length > 0) {
      setConversations(convs);
      // agent 状态异步填充 picker，不挡列表
      void listAgents()
        .then(setAgentStatus)
        .catch(() => {});
      return convs;
    }
    const agents = await listAgents();
    setAgentStatus(agents);
    const ensured = await ensureConversation(convs, agents);
    setConversations(ensured);
    return ensured;
  }, [ensureConversation]);

  const loadMessages = useCallback(async (id: string) => {
    return listChatMessages(id);
  }, []);

  const loadProviders = useCallback(async (agentId: AgentId) => {
    try {
      const list = await listProviders(agentId);
      setProviders(list);
    } catch {
      setProviders([]);
    }
  }, []);

  useEffect(() => {
    setListLoading(true);
    setError(null);
    loadList()
      .then(async (convs) => {
        // Projects → Chat：新建会话并预填（可选自动发送）提示
        const fromProjects = searchParams.get('from') === 'projects';
        if (fromProjects && !bootstrapDoneRef.current) {
          bootstrapDoneRef.current = true;
          const boot = takeChatBootstrap();
          // 清掉 query，避免刷新重复创建
          setSearchParams({}, { replace: true });
          if (boot) {
            try {
              const created = await createConversation(boot.agentIds, boot.cwd ?? null);
              let next = created;
              if (boot.title) {
                try {
                  next = await updateConversation(created.id, { title: boot.title });
                } catch {
                  /* title 可选 */
                }
              }
              setConversations((prev) => [next, ...prev.filter((c) => c.id !== next.id)]);
              setActiveId(next.id);
              setMessages([]);
              if (boot.prompt?.trim()) {
                setDraft(boot.prompt);
                toast({
                  title: '已从 Projects 创建会话',
                  description: next.cwd
                    ? '提示词已填入；确认 cwd 后发送即可。'
                    : '提示词已填入；建议先设置工作目录再发送。',
                  variant: 'success',
                });
                if (!next.cwd) setSettingsOpen(true);
              }
              return;
            } catch (e) {
              toast({
                title: e instanceof Error ? e.message : String(e),
                variant: 'danger',
              });
            }
          }
        }
        setActiveId(convs[0]?.id ?? null);
      })
      .catch(setError)
      .finally(() => setListLoading(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps -- bootstrap once on mount/list load
  }, [loadList]);

  // messages 与 providers 独立并发（不再串在 loadList 之后的瀑布里）
  useEffect(() => {
    // 过程面板仅内存；切会话时清空，避免串台
    setProcessMap({});
    streamingRef.current = {};
    if (!activeId) {
      setMessages([]);
      return;
    }
    let cancelled = false;
    setMessagesLoading(true);
    loadMessages(activeId)
      .then((rows) => {
        if (!cancelled && activeIdRef.current === activeId) {
          setMessages(rows);
        }
      })
      .catch((e) => {
        if (!cancelled) setError(e);
      })
      .finally(() => {
        if (!cancelled) setMessagesLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [activeId, loadMessages]);

  useEffect(() => {
    if (!primaryAgent) {
      setProviders([]);
      return;
    }
    void loadProviders(primaryAgent);
  }, [primaryAgent, loadProviders]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: 'nearest' });
  }, [messages, sending]);

  const turns = useMemo(() => groupByTurn(messages), [messages]);

  async function handleNewChat() {
    const agents = defaultAgents(agentStatus);
    try {
      const conv = await createConversation(agents, active?.cwd ?? null);
      setConversations((prev) => [conv, ...prev]);
      setActiveId(conv.id);
      setMessages([]);
      setDraft('');
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    }
  }

  async function handleDelete(id: string) {
    try {
      if (sendingConversationId === id) {
        await chatCancel(id).catch(() => {});
      }
      await deleteConversation(id);
      const rest = conversations.filter((c) => c.id !== id);
      if (rest.length === 0) {
        const created = await createConversation(defaultAgents(agentStatus), active?.cwd ?? null);
        setConversations([created]);
        setActiveId(created.id);
        setMessages([]);
        setDraft('');
        return;
      }
      setConversations(rest);
      if (activeId === id) {
        setActiveId(rest[0].id);
        setMessages([]);
      }
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    }
  }

  async function patchActive(patch: Parameters<typeof updateConversation>[1]) {
    if (!active) return;
    try {
      const updated = await updateConversation(active.id, patch);
      setConversations((prev) => prev.map((c) => (c.id === updated.id ? updated : c)));
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    }
  }

  async function toggleConversationAgent(id: AgentId) {
    if (!active || sending) return;
    if (installed.get(id) === false) return;
    const set = new Set(active.agentIds);
    if (set.has(id)) {
      if (set.size === 1) {
        toast({ title: '至少保留一个 Agent', variant: 'danger' });
        return;
      }
      set.delete(id);
    } else {
      set.add(id);
    }
    const next = AGENT_IDS.filter((a) => set.has(a));
    await patchActive({ agentIds: next });
  }

  async function handleSwitchProvider(providerId: string) {
    if (!primaryAgent || switchingProvider) return;
    setSwitchingProvider(true);
    try {
      await switchProvider(primaryAgent, providerId);
      await loadProviders(primaryAgent);
      toast({ title: '已切换连接', variant: 'success' });
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      setSwitchingProvider(false);
    }
  }

  function applyEvent(ev: ChatEvent, sendConvId: string) {
    if (activeIdRef.current !== sendConvId) {
      if (ev.type === 'error') toast({ title: ev.message, variant: 'danger' });
      return;
    }
    // 过程面板状态（命令 / stderr / 细状态）与 messages 并行维护
    setProcessMap((prev) => reduceProcessEvent(prev, ev));

    if (ev.type === 'started') {
      streamingRef.current = {};
      for (const agent of ev.agents) {
        streamingRef.current[processKey(ev.turn, agent)] = '';
        setMessages((prev) => {
          const hasRunning = prev.some(
            (m) => m.turn === ev.turn && m.role === 'agent' && m.agentId === agent,
          );
          if (hasRunning) return prev;
          return [
            ...prev,
            {
              id: `local-${ev.turn}-${agent}`,
              conversationId: sendConvId,
              turn: ev.turn,
              role: 'agent',
              agentId: agent,
              content: '',
              status: 'running',
              durationMs: 0,
              createdAt: new Date().toISOString(),
            },
          ];
        });
      }
      return;
    }
    if (ev.type === 'agentChunk' && ev.stream === 'stdout') {
      const key = processKey(ev.turn, ev.agent);
      streamingRef.current[key] = (streamingRef.current[key] ?? '') + ev.text;
      const content = streamingRef.current[key];
      setMessages((prev) =>
        prev.map((m) =>
          m.role === 'agent' &&
          m.agentId === ev.agent &&
          m.turn === ev.turn &&
          m.status === 'running'
            ? { ...m, content }
            : m,
        ),
      );
      return;
    }
    if (ev.type === 'agentFinished') {
      setMessages((prev) => {
        const withoutLocal = prev.filter(
          (m) =>
            !(
              m.role === 'agent' &&
              m.agentId === ev.agent &&
              m.turn === ev.turn &&
              (m.status === 'running' || m.id.startsWith('local-'))
            ),
        );
        return [...withoutLocal, ev.message];
      });
      return;
    }
    if (ev.type === 'error') {
      toast({ title: ev.message, variant: 'danger' });
    }
  }

  async function handleSend() {
    if (!active || sending) return;
    const prompt = draft.trim();
    if (!prompt) return;
    if (!active.cwd) {
      toast({
        title: '请先设置工作目录',
        description: '点击输入框旁的设置，填写 cwd',
        variant: 'danger',
      });
      setSettingsOpen(true);
      return;
    }

    const sendConvId = active.id;
    setSending(true);
    setSendingConversationId(sendConvId);
    setDraft('');
    const turnGuess = messages.reduce((max, m) => Math.max(max, m.turn), 0) + 1;
    setMessages((prev) => [
      ...prev,
      {
        id: `local-user-${Date.now()}`,
        conversationId: sendConvId,
        turn: turnGuess,
        role: 'user',
        content: prompt,
        status: 'ok',
        durationMs: 0,
        createdAt: new Date().toISOString(),
      },
    ]);

    try {
      await chatSend(sendConvId, prompt, (ev) => applyEvent(ev, sendConvId));
      const convs = await listConversations();
      setConversations(convs);
      if (activeIdRef.current === sendConvId) {
        setMessages(await listChatMessages(sendConvId));
      }
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
      if (activeIdRef.current === sendConvId) {
        const rows = await loadMessages(sendConvId).catch(() => null);
        if (rows && activeIdRef.current === sendConvId) setMessages(rows);
      }
    } finally {
      setSending(false);
      setSendingConversationId(null);
    }
  }

  async function handleCancel() {
    if (!sendingConversationId) return;
    try {
      await chatCancel(sendingConversationId);
      toast({
        title: '已请求取消',
        description: '正在停止当前生成，过程面板将显示已取消。',
        variant: 'success',
        duration: 4000,
      });
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    }
  }

  const agentPickerLabel = useMemo(() => {
    if (!active) return '选择 Agent';
    if (active.agentIds.length === 1) return AGENT_MAP[active.agentIds[0]].name;
    return `${active.agentIds.length} 个 Agent`;
  }, [active]);

  const modelPickerLabel = useMemo(() => {
    if (!primaryAgent) return '切换连接';
    if (switchingProvider) return '切换中…';
    if (!currentProvider) return '未配置连接';
    return currentProvider.name;
  }, [primaryAgent, currentProvider, switchingProvider]);

  const modelPickerSubtitle = useMemo(() => {
    if (!currentProvider || switchingProvider) return null;
    return extractModel(currentProvider.configText);
  }, [currentProvider, switchingProvider]);

  if (error && conversations.length === 0 && !listLoading) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <ErrorState
          error={error}
          onRetry={() => {
            setListLoading(true);
            setError(null);
            loadList()
              .then((c) => c[0] && setActiveId(c[0].id))
              .catch(setError)
              .finally(() => setListLoading(false));
          }}
        />
      </div>
    );
  }

  const messageStatusLabel = (status: string, process?: AgentProcessView) => {
    // 过程机更细（排队/启动）；终态以 message.status 为准
    if (process && (status === 'running' || !status)) {
      if (process.phase === 'queued' || process.phase === 'starting' || process.phase === 'running') {
        return processPhaseLabel(process.phase);
      }
    }
    switch (status) {
      case 'running':
        return '生成中';
      case 'error':
      case 'failed':
        return '失败';
      case 'cancelled':
        return '已取消';
      case 'timeout':
        return '超时';
      case 'ok':
      case 'done':
      case 'success':
        return null; // 成功态不显示
      default:
        return status;
    }
  };

  return (
    <div className="flex h-full min-h-0 bg-canvas">
      {/* 左侧会话 rail：画布色，与主区白底分层 */}
      <aside
        className={cn(
          'flex shrink-0 flex-col border-r border-border bg-canvas transition-[width] duration-200',
          railOpen ? 'w-56' : 'w-0 overflow-hidden border-r-0',
        )}
      >
        <div className="flex items-center gap-1.5 p-2">
          <Hint label="收起历史">
            <button
              type="button"
              className="flex h-7 w-7 shrink-0 items-center justify-center rounded-btn text-muted hover:bg-hover hover:text-primary"
              onClick={() => setRailOpen(false)}
              aria-label="收起历史"
            >
              <PanelLeftClose className="h-4 w-4" />
            </button>
          </Hint>
          <Button className="min-w-0 flex-1 justify-start gap-1.5" size="sm" variant="secondary" onClick={() => void handleNewChat()}>
            <Plus className="h-3.5 w-3.5" />
            新建对话
          </Button>
        </div>
        <div className="px-3 pb-1.5 text-2xs font-medium uppercase tracking-wide text-muted">
          对话历史
        </div>
        <div className="flex-1 overflow-y-auto px-1.5 pb-3">
          {listLoading ? (
            <div className="space-y-2 px-1 pt-1">
              {Array.from({ length: 5 }).map((_, i) => (
                <Skeleton key={i} className="h-10 w-full rounded-btn" />
              ))}
            </div>
          ) : conversations.length === 0 ? (
            <div className="px-2 py-4 text-center">
              <p className="text-xs text-muted">暂无对话</p>
              <p className="mt-1 text-2xs text-muted">点上方「新建对话」开始</p>
            </div>
          ) : (
            conversations.map((c) => {
              const selected = activeId === c.id;
              return (
                <div
                  key={c.id}
                  className={cn(
                    'group mb-0.5 flex items-center rounded-btn',
                    selected ? 'bg-hover' : 'hover:bg-hover/70',
                  )}
                >
                  <button
                    type="button"
                    onClick={() => setActiveId(c.id)}
                    className={cn(
                      'min-w-0 flex-1 px-2 py-1.5 text-left text-sm',
                      selected ? 'font-medium text-primary' : 'text-secondary',
                    )}
                  >
                    <span className="flex items-center gap-2">
                      <MessagesSquare className="h-3.5 w-3.5 shrink-0 opacity-50" />
                      <span className="truncate">{c.title || '新对话'}</span>
                    </span>
                    <span className="mt-0.5 block pl-5 text-xs text-muted">
                      {relativeTime(c.updatedAt)}
                    </span>
                  </button>
                  <Hint label="删除">
                    <button
                      type="button"
                      className="mr-1 rounded-btn p-1 opacity-0 transition-opacity hover:bg-panel group-hover:opacity-100"
                      aria-label="删除"
                      onClick={() => void handleDelete(c.id)}
                    >
                      <Trash2 className="h-3.5 w-3.5 text-muted hover:text-danger" />
                    </button>
                  </Hint>
                </div>
              );
            })
          )}
        </div>
      </aside>

      {/* 右侧：对话窗口 */}
      <section className="relative flex min-w-0 flex-1 flex-col bg-panel">
        <header className="flex h-10 shrink-0 items-center gap-2 border-b border-border px-4">
          {!railOpen && (
            <Hint label="展开历史">
              <button
                type="button"
                className="flex h-7 w-7 items-center justify-center rounded-btn text-muted hover:bg-hover hover:text-primary"
                onClick={() => setRailOpen(true)}
                aria-label="展开历史"
              >
                <PanelLeftOpen className="h-4 w-4" />
              </button>
            </Hint>
          )}
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-sm font-semibold text-primary">
              {active?.title || (active ? '新对话' : '对话')}
            </h1>
          </div>
          {active && (
            <Hint label="会话设置">
              <button
                type="button"
                className="flex h-7 w-7 items-center justify-center rounded-btn text-muted hover:bg-hover hover:text-primary"
                aria-label="会话设置"
                onClick={() => setSettingsOpen(true)}
              >
                <Settings2 className="h-4 w-4" />
              </button>
            </Hint>
          )}
        </header>

        {listLoading && !active ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6">
            <ListSkeleton rows={4} className="w-full max-w-2xl" />
          </div>
        ) : null}

        {active && (
          <>
            {/* 消息区可滚；composer 始终贴底，避免长文把输入框顶出视口 */}
            <div className="min-h-0 flex-1 overflow-y-auto">
              {messagesLoading && turns.length === 0 ? (
                <div className="flex h-full flex-col justify-center p-6">
                  <ListSkeleton rows={3} className="mx-auto w-full max-w-2xl" />
                </div>
              ) : turns.length === 0 ? (
                <div className="flex h-full flex-col items-center justify-center px-6 py-10">
                  <div className="text-center">
                    <p className="text-xl font-semibold tracking-tight text-primary">
                      开始对话
                    </p>
                    <p className="mt-2 max-w-md text-sm text-muted">
                      选择 Agent 与连接后输入；多选可并排对比
                    </p>
                  </div>
                </div>
              ) : (
                <div className="mx-auto w-full max-w-3xl space-y-6 px-4 py-6">
                  {turns.map((g) => (
                    <div key={g.turn} className="space-y-4">
                      {g.user && (
                        <div className="flex justify-end">
                          <div className="max-w-[85%] rounded-composer bg-subtle px-4 py-2 text-sm text-primary">
                            {/* User prompts are usually plain text; markdown still harmless. */}
                            <MarkdownView content={g.user.content} variant="chat" />
                          </div>
                        </div>
                      )}
                      {g.agents.map((m) => {
                        const agent = m.agentId ?? 'claude';
                        const proc = processMap[processKey(m.turn, agent)];
                        const statusText = messageStatusLabel(m.status, proc);
                        return (
                          <div key={m.id} className="flex gap-3">
                            <AgentLogo agentId={agent} size="md" />
                            <div className="min-w-0 flex-1 pt-0.5">
                              <div className="mb-1 flex items-center gap-2 text-xs text-muted">
                                <span className="font-medium text-secondary">
                                  {agentDisplayName(agent)}
                                </span>
                                {statusText && <span>{statusText}</span>}
                                {m.durationMs > 0 && (
                                  <span>{formatDurationMs(m.durationMs)}</span>
                                )}
                              </div>
                              {hasProcessDetails(proc) && proc ? (
                                <AgentProcessPanel
                                  view={proc}
                                  messageStatus={m.status}
                                  durationMs={m.durationMs}
                                  exitCode={m.exitCode}
                                />
                              ) : null}
                              <div className="text-sm leading-relaxed text-primary">
                                {m.content ? (
                                  <MarkdownView content={m.content} variant="chat" />
                                ) : m.status === 'running' ? (
                                  <span className="inline-flex items-center gap-2 text-muted">
                                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                                    {proc
                                      ? `${processPhaseLabel(proc.phase)}…`
                                      : '生成中…'}
                                  </span>
                                ) : (
                                  <span className="text-muted">{m.error || '（无输出）'}</span>
                                )}
                                {m.error && m.status !== 'ok' && m.content && (
                                  <p className="mt-2 text-sm text-danger">{m.error}</p>
                                )}
                              </div>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  ))}
                  <div ref={bottomRef} />
                </div>
              )}
            </div>

            <div className="shrink-0 border-t border-border/60 bg-canvas px-4 pb-4 pt-2">
              <div className="mx-auto w-full max-w-3xl">
                <ChatComposer
                  draft={draft}
                  setDraft={setDraft}
                  sending={sending}
                  active={active}
                  installed={installed}
                  providers={providers}
                  primaryAgent={primaryAgent}
                  agentPickerLabel={agentPickerLabel}
                  modelPickerLabel={modelPickerLabel}
                  modelPickerSubtitle={modelPickerSubtitle}
                  switchingProvider={switchingProvider}
                  onSend={() => void handleSend()}
                  onCancel={() => void handleCancel()}
                  onToggleAgent={(id) => void toggleConversationAgent(id)}
                  onSwitchProvider={(id) => void handleSwitchProvider(id)}
                />
              </div>
            </div>
          </>
        )}
        {!active && !listLoading && (
          <div className="flex flex-1 items-center justify-center p-6">
            <EmptyState
              icon={MessagesSquare}
              title="未选择对话"
              description="选历史，或新建"
              actionLabel="新建"
              onAction={() => void handleNewChat()}
            />
          </div>
        )}
      </section>

      {/* 会话设置：cwd / 自动批准 */}
      <Dialog open={settingsOpen} onOpenChange={setSettingsOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>会话设置</DialogTitle>
            <DialogDescription>工作目录与自动批准</DialogDescription>
          </DialogHeader>
          {active && (
            <div className="space-y-4 py-2">
              <div>
                <label className="mb-1.5 flex items-center gap-1.5 text-xs text-muted">
                  <FolderOpen className="h-3.5 w-3.5" />
                  工作目录
                </label>
                <Input
                  placeholder="例如 D:\\projects\\demo"
                  defaultValue={active.cwd ?? ''}
                  onBlur={(e) => {
                    const v = e.target.value.trim();
                    void patchActive({ cwd: v || null });
                  }}
                />
              </div>
              <label className="flex items-center justify-between gap-3 text-sm">
                <span>
                  <span className="block font-medium">自动批准</span>
                  <Tip className="text-xs text-muted" label="关闭时 CLI 遇审批可能等到超时">
                    跳过工具确认
                  </Tip>
                </span>
                <Switch
                  checked={active.allowDangerous}
                  onCheckedChange={(checked) => {
                    if (checked) {
                      setDangerConfirm(true);
                      return;
                    }
                    void patchActive({ allowDangerous: false });
                  }}
                />
              </label>
            </div>
          )}
          <DialogFooter>
            <Button onClick={() => setSettingsOpen(false)}>完成</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={dangerConfirm} onOpenChange={setDangerConfirm}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>开启自动批准？</DialogTitle>
            <DialogDescription>
              开启后将跳过工具确认，Agent 可直接改文件、执行命令。仅在信任当前工作目录时开启。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setDangerConfirm(false)}>
              取消
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                setDangerConfirm(false);
                void patchActive({ allowDangerous: true });
              }}
            >
              确认开启
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
