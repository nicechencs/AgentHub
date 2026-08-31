/**
 * 连接池页右上角的接入入口：先在浮动页面中选择接入方式，再打开对应的配置页面。
 * OAuth 只提供官方登录支持的三个 Agent；API 接入按下游接口类型提供固定选项。
 * 这里接入的登录只给连接池用，不会出现在连接页。
 */
import { useMemo, useState, type ReactNode } from 'react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { useI18n } from '@/components/shared/LanguageProvider';
import { OAuthFlowDialog } from '@/components/connect/OAuthFlowDialog';
import { ProviderEditDialog } from '@/components/connections/ProviderEditDialog';
import { SecretInput } from '@/components/shared/SecretInput';
import { RouteEndpointTypeText } from '@/components/shared/RouteEndpointUrl';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { useToast } from '@/components/ui/toast';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import type { AgentId } from '@/lib/types';
import { attachPoolOwnedAuthorization, syncConnectionAuthorizations } from '@/lib/api/adapter';
import { detectApiEndpointTypes } from '@/lib/api/provider';
import type { AdapterSourceKind, DetectedApiEndpointType, RoutePoolSurface } from '@/lib/backend/contracts';
import { cn } from '@/lib/utils';

export type PoolAccessAgent = 'claude' | 'codex' | 'grok';

export type PoolOAuthChoice = {
  agentId: PoolAccessAgent;
  available: boolean;
};

export type PoolApiChoice = {
  agentId: PoolAccessAgent;
  type: 'claudeMessages' | 'openaiResponses' | 'grokResponses' | 'openaiChatCompletions';
  endpoint: '/v1/messages' | '/v1/responses' | '/v1/chat/completions';
  grokApiBackend?: 'responses' | 'chat_completions';
  available: boolean;
};

const OAUTH_AGENTS = ['claude', 'codex', 'grok'] as const satisfies readonly PoolAccessAgent[];

const API_CHOICES = [
  { agentId: 'claude', type: 'claudeMessages', endpoint: '/v1/messages' },
  { agentId: 'codex', type: 'openaiResponses', endpoint: '/v1/responses' },
  { agentId: 'grok', type: 'grokResponses', endpoint: '/v1/responses', grokApiBackend: 'responses' },
  {
    agentId: 'grok',
    type: 'openaiChatCompletions',
    endpoint: '/v1/chat/completions',
    grokApiBackend: 'chat_completions',
  },
] as const satisfies readonly {
  agentId: PoolAccessAgent;
  type: PoolApiChoice['type'];
  endpoint: PoolApiChoice['endpoint'];
  grokApiBackend?: PoolApiChoice['grokApiBackend'];
}[];

const DETECTED_API_CHOICES: Record<DetectedApiEndpointType, readonly PoolApiChoice['type'][]> = {
  messages: ['claudeMessages'],
  responses: ['openaiResponses', 'grokResponses'],
  chat_completions: ['openaiChatCompletions'],
};

type ApiEndpointPreset = {
  id: string;
  labelKey:
    | 'routes.pool.page.apiVendorAnthropic'
    | 'routes.pool.page.apiVendorZhipuBigModel'
    | 'routes.pool.page.apiVendorKimiCn'
    | 'routes.pool.page.apiVendorKimiGlobal'
    | 'routes.pool.page.apiVendorOpenai'
    | 'routes.pool.page.apiVendorDeepseek'
    | 'routes.pool.page.apiVendorXai'
    | 'routes.pool.page.apiVendorQwenCn'
    | 'routes.pool.page.apiVendorQwenSingapore'
    | 'routes.pool.page.apiVendorQwenUs'
    | 'routes.pool.page.apiVendorZhipuZai'
    | 'routes.pool.page.apiVendorOpenRouter'
    | 'routes.pool.page.apiVendorNvidia'
    | 'routes.pool.page.apiVendorGroq'
    | 'routes.pool.page.apiVendorGemini'
    | 'routes.pool.page.apiVendorMistral';
  baseUrl: string;
};

// 只列出该端点实际兼容的厂商，避免「选了厂商却请求到不支持的接口」。
// 同一厂商的不同地区使用独立选项，便于直接带出对应的服务地址。
const API_ENDPOINT_PRESETS: Record<PoolApiChoice['type'], readonly ApiEndpointPreset[]> = {
  claudeMessages: [
    { id: 'anthropic', labelKey: 'routes.pool.page.apiVendorAnthropic', baseUrl: 'https://api.anthropic.com' },
    { id: 'qwen-cn', labelKey: 'routes.pool.page.apiVendorQwenCn', baseUrl: 'https://dashscope.aliyuncs.com/apps/anthropic' },
    { id: 'qwen-sg', labelKey: 'routes.pool.page.apiVendorQwenSingapore', baseUrl: 'https://dashscope-intl.aliyuncs.com/apps/anthropic' },
    { id: 'qwen-us', labelKey: 'routes.pool.page.apiVendorQwenUs', baseUrl: 'https://dashscope-us.aliyuncs.com/apps/anthropic' },
    { id: 'zhipu-bigmodel', labelKey: 'routes.pool.page.apiVendorZhipuBigModel', baseUrl: 'https://open.bigmodel.cn/api/anthropic' },
    { id: 'zhipu-zai', labelKey: 'routes.pool.page.apiVendorZhipuZai', baseUrl: 'https://api.z.ai/api/anthropic' },
    { id: 'deepseek', labelKey: 'routes.pool.page.apiVendorDeepseek', baseUrl: 'https://api.deepseek.com/anthropic' },
    { id: 'kimi-cn', labelKey: 'routes.pool.page.apiVendorKimiCn', baseUrl: 'https://api.moonshot.cn/anthropic' },
    { id: 'kimi-global', labelKey: 'routes.pool.page.apiVendorKimiGlobal', baseUrl: 'https://api.moonshot.ai/anthropic' },
    { id: 'openrouter', labelKey: 'routes.pool.page.apiVendorOpenRouter', baseUrl: 'https://openrouter.ai/api/v1' },
  ],
  openaiResponses: [
    { id: 'openai', labelKey: 'routes.pool.page.apiVendorOpenai', baseUrl: 'https://api.openai.com/v1' },
    { id: 'deepseek', labelKey: 'routes.pool.page.apiVendorDeepseek', baseUrl: 'https://api.deepseek.com' },
    { id: 'openrouter', labelKey: 'routes.pool.page.apiVendorOpenRouter', baseUrl: 'https://openrouter.ai/api/v1' },
  ],
  grokResponses: [
    { id: 'xai', labelKey: 'routes.pool.page.apiVendorXai', baseUrl: 'https://api.x.ai/v1' },
    { id: 'openrouter', labelKey: 'routes.pool.page.apiVendorOpenRouter', baseUrl: 'https://openrouter.ai/api/v1' },
  ],
  openaiChatCompletions: [
    { id: 'openai', labelKey: 'routes.pool.page.apiVendorOpenai', baseUrl: 'https://api.openai.com/v1' },
    { id: 'qwen-cn', labelKey: 'routes.pool.page.apiVendorQwenCn', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1' },
    { id: 'qwen-sg', labelKey: 'routes.pool.page.apiVendorQwenSingapore', baseUrl: 'https://dashscope-intl.aliyuncs.com/compatible-mode/v1' },
    { id: 'qwen-us', labelKey: 'routes.pool.page.apiVendorQwenUs', baseUrl: 'https://dashscope-us.aliyuncs.com/compatible-mode/v1' },
    { id: 'zhipu-bigmodel', labelKey: 'routes.pool.page.apiVendorZhipuBigModel', baseUrl: 'https://open.bigmodel.cn/api/paas/v4' },
    { id: 'zhipu-zai', labelKey: 'routes.pool.page.apiVendorZhipuZai', baseUrl: 'https://api.z.ai/api/paas/v4' },
    { id: 'deepseek', labelKey: 'routes.pool.page.apiVendorDeepseek', baseUrl: 'https://api.deepseek.com' },
    { id: 'kimi-cn', labelKey: 'routes.pool.page.apiVendorKimiCn', baseUrl: 'https://api.moonshot.cn/v1' },
    { id: 'kimi-global', labelKey: 'routes.pool.page.apiVendorKimiGlobal', baseUrl: 'https://api.moonshot.ai/v1' },
    { id: 'xai', labelKey: 'routes.pool.page.apiVendorXai', baseUrl: 'https://api.x.ai/v1' },
    { id: 'gemini', labelKey: 'routes.pool.page.apiVendorGemini', baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai/' },
    { id: 'mistral', labelKey: 'routes.pool.page.apiVendorMistral', baseUrl: 'https://api.mistral.ai/v1' },
    { id: 'openrouter', labelKey: 'routes.pool.page.apiVendorOpenRouter', baseUrl: 'https://openrouter.ai/api/v1' },
    { id: 'nvidia', labelKey: 'routes.pool.page.apiVendorNvidia', baseUrl: 'https://integrate.api.nvidia.com/v1' },
    { id: 'groq', labelKey: 'routes.pool.page.apiVendorGroq', baseUrl: 'https://api.groq.com/openai/v1' },
  ],
};

/** Maps the fixed OAuth choices to their current installed/supported state. */
export function poolOAuthChoices(
  agents: readonly AgentId[],
  oauthAgents: readonly AgentId[],
): PoolOAuthChoice[] {
  return OAUTH_AGENTS.map((agentId) => ({
    agentId,
    available: agents.includes(agentId) && oauthAgents.includes(agentId),
  }));
}

/** Maps the fixed API endpoint choices to their current installed state. */
export function poolApiChoices(agents: readonly AgentId[]): PoolApiChoice[] {
  return API_CHOICES.map((choice) => ({
    ...choice,
    available: agents.includes(choice.agentId),
  }));
}

export function poolSurfaceForOAuth(agentId: PoolAccessAgent): RoutePoolSurface {
  return agentId === 'claude' ? 'messages' : 'responses';
}

export function poolSurfaceForApiChoice(
  choice: Pick<PoolApiChoice, 'endpoint'>,
): RoutePoolSurface {
  if (choice.endpoint === '/v1/messages') return 'messages';
  if (choice.endpoint === '/v1/chat/completions') return 'chat_completions';
  return 'responses';
}

type ChoiceDialogProps = {
  open: boolean;
  title: string;
  description: string;
  children: ReactNode;
  choicesClassName?: string;
  onOpenChange: (open: boolean) => void;
};

function ChoiceDialog({
  open,
  title,
  description,
  children,
  choicesClassName,
  onOpenChange,
}: ChoiceDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <div className={cn('grid grid-cols-1 gap-2', choicesClassName)}>{children}</div>
      </DialogContent>
    </Dialog>
  );
}

function OAuthChoiceCard({
  label,
  unavailableLabel,
  agentId,
  available,
  onClick,
}: {
  label: string;
  unavailableLabel: string;
  agentId: PoolAccessAgent;
  available: boolean;
  onClick: () => void;
}) {
  return (
    <div
      role="button"
      tabIndex={available ? 0 : -1}
      aria-disabled={!available}
      onClick={() => {
        if (available) onClick();
      }}
      onKeyDown={(event) => {
        if (!available) return;
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onClick();
        }
      }}
      className={cn(
        'rounded-card border border-border bg-panel p-3 text-left transition-colors',
        available && 'hover:bg-hover/50',
        !available && 'opacity-60',
      )}
    >
      <div className="flex items-center gap-2">
        <AgentLogo agentId={agentId} size="sm" />
        <span className="text-sm font-medium">{label}</span>
      </div>
      {!available ? <p className="mt-1 text-xs text-muted">{unavailableLabel}</p> : null}
    </div>
  );
}

function ApiChoiceCard({
  label,
  unavailableLabel,
  endpoint,
  available,
  onClick,
}: {
  label: string;
  unavailableLabel: string;
  endpoint: PoolApiChoice['endpoint'];
  available: boolean;
  onClick: () => void;
}) {
  const endpointId =
    endpoint === '/v1/messages'
      ? 'messages'
      : endpoint === '/v1/chat/completions'
        ? 'chat_completions'
        : 'responses';
  return (
    <div
      role="button"
      tabIndex={available ? 0 : -1}
      aria-disabled={!available}
      onClick={() => {
        if (available) onClick();
      }}
      onKeyDown={(event) => {
        if (!available) return;
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onClick();
        }
      }}
      className={cn(
        'rounded-card border border-border bg-panel p-3 text-left transition-colors',
        available && 'hover:bg-hover/50',
        !available && 'opacity-60',
      )}
    >
      <div className="min-w-0">
        <p className="font-mono text-sm font-medium text-primary">{endpoint}</p>
        <p className="text-xs">
          <RouteEndpointTypeText endpointId={endpointId}>{label}</RouteEndpointTypeText>
        </p>
      </div>
      {!available ? <p className="mt-1 text-xs text-muted">{unavailableLabel}</p> : null}
    </div>
  );
}

function ApiDetectDialog({
  open,
  onOpenChange,
  onSelect,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (choice: PoolApiChoice, credentials: { baseUrl: string; apiKey: string }) => void;
}) {
  const { t } = useI18n();
  const [baseUrl, setBaseUrl] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [detecting, setDetecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [types, setTypes] = useState<DetectedApiEndpointType[]>([]);
  const choices = API_CHOICES.filter((choice) =>
    types.some((type) => DETECTED_API_CHOICES[type].includes(choice.type)),
  );

  const detect = async () => {
    setDetecting(true);
    setError(null);
    setTypes([]);
    try {
      const detected = await detectApiEndpointTypes(baseUrl, apiKey);
      setTypes(detected);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setDetecting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t('routes.pool.page.apiDetectTitle')}</DialogTitle>
          <DialogDescription>{t('routes.pool.page.apiDetectDescription')}</DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-3">
          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">{t('connections.providerDialog.endpoint')}</span>
            <Input
              value={baseUrl}
              onChange={(event) => setBaseUrl(event.target.value)}
              placeholder="https://api.example.com/v1"
              autoComplete="off"
              spellCheck={false}
            />
          </label>
          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">{t('connections.apiKeyDialog.key')}</span>
            <SecretInput value={apiKey} onChange={setApiKey} />
          </label>
          <Button type="button" onClick={() => void detect()} disabled={detecting || !baseUrl || !apiKey}>
            {detecting ? t('routes.pool.page.apiDetecting') : t('routes.pool.page.apiDetect')}
          </Button>
          {error ? <p className="text-meta text-danger">{error}</p> : null}
          {types.length === 0 && !detecting && !error ? null : choices.length > 0 ? (
            <div className="grid grid-cols-1 gap-2">
              {choices.map((choice) => (
                <ApiChoiceCard
                  key={`${choice.agentId}-${choice.endpoint}`}
                  available
                  endpoint={choice.endpoint}
                  label={t(apiLabelKeys[choice.type])}
                  unavailableLabel=""
                  onClick={() => onSelect({ ...choice, available: true }, { baseUrl, apiKey })}
                />
              ))}
            </div>
          ) : (
            <p className="text-meta text-muted">{t('routes.pool.page.apiDetectNone')}</p>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

const oauthLabelKeys = {
  claude: 'routes.pool.page.oauthClaude',
  codex: 'routes.pool.page.oauthCodex',
  grok: 'routes.pool.page.oauthGrok',
} as const;

const apiLabelKeys = {
  claudeMessages: 'routes.pool.page.apiClaude',
  openaiResponses: 'routes.pool.page.apiCodex',
  grokResponses: 'routes.pool.page.apiGrok',
  openaiChatCompletions: 'routes.pool.page.apiOpenaiChatCompletions',
} as const;

export function PoolAddButtons({
  agents,
  oauthAgents,
  onChanged,
}: {
  agents: readonly AgentId[];
  oauthAgents: readonly AgentId[];
  /** Called after an OAuth flow or API provider is saved. */
  onChanged?: () => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [picker, setPicker] = useState<'oauth' | 'api' | null>(null);
  const [apiDetectOpen, setApiDetectOpen] = useState(false);
  const [oauthAgentId, setOauthAgentId] = useState<PoolAccessAgent | null>(null);
  const [apiChoice, setApiChoice] = useState<PoolApiChoice | null>(null);
  const [apiCredentials, setApiCredentials] = useState<{ baseUrl: string; apiKey: string } | null>(null);
  const [syncOpen, setSyncOpen] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const oauthChoices = useMemo(
    () => poolOAuthChoices(agents, oauthAgents),
    [agents, oauthAgents],
  );
  const apiChoices = useMemo(() => poolApiChoices(agents), [agents]);
  const attachAuthorization = async (
    sourceKind: AdapterSourceKind,
    sourceId: string,
    targetAgentId: PoolAccessAgent,
    surface: RoutePoolSurface,
  ) => {
    try {
      await attachPoolOwnedAuthorization({
        sourceKind,
        sourceId,
        targetAgentId,
        surface,
      });
      toast({ title: t('routes.pool.page.added'), variant: 'success' });
      onChanged?.();
    } catch (error) {
      toast({
        title: t('routes.pool.page.addFailed'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    }
  };

  const syncFromConnections = async () => {
    setSyncing(true);
    try {
      const result = await syncConnectionAuthorizations();
      setSyncOpen(false);
      toast({
        title: result.added > 0
          ? t('routes.pool.page.synced', { count: result.added })
          : t('routes.pool.page.syncNone'),
        variant: 'success',
      });
      onChanged?.();
    } catch (error) {
      toast({
        title: t('routes.pool.page.syncFailed'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      setSyncing(false);
    }
  };

  const compactEndpointPresets = apiChoice
    ? API_ENDPOINT_PRESETS[apiChoice.type].map((preset) => ({
        id: preset.id,
        label: t(preset.labelKey),
        baseUrl: preset.baseUrl,
      }))
    : undefined;

  const selectOAuthAgent = (agentId: PoolAccessAgent) => {
    setPicker(null);
    setOauthAgentId(agentId);
  };
  const selectApiChoice = (choice: PoolApiChoice) => {
    setPicker(null);
    setApiCredentials(null);
    setApiChoice(choice);
  };
  const selectDetectedApiChoice = (
    choice: PoolApiChoice,
    credentials: { baseUrl: string; apiKey: string },
  ) => {
    setApiDetectOpen(false);
    setApiCredentials(credentials);
    setApiChoice(choice);
  };

  return (
    <>
      <Button
        type="button"
        size="sm"
        variant="secondary"
        onClick={() => setPicker('oauth')}
      >
        {t('routes.pool.page.addOauth')}
      </Button>
      <Button
        type="button"
        size="sm"
        variant="secondary"
        onClick={() => setPicker('api')}
      >
        {t('routes.pool.page.addApiKey')}
      </Button>
      <Button
        type="button"
        size="sm"
        variant="secondary"
        onClick={() => setSyncOpen(true)}
      >
        {t('routes.pool.page.syncFromConnections')}
      </Button>

      <Dialog open={syncOpen} onOpenChange={(open) => { if (!syncing) setSyncOpen(open); }}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{t('routes.pool.page.syncTitle')}</DialogTitle>
            <DialogDescription>{t('routes.pool.page.syncDescription')}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              type="button"
              variant="secondary"
              onClick={() => setSyncOpen(false)}
              disabled={syncing}
            >
              {t('common.cancel')}
            </Button>
            <Button type="button" onClick={() => void syncFromConnections()} disabled={syncing}>
              {syncing ? t('routes.pool.page.syncing') : t('routes.pool.page.syncConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ChoiceDialog
        open={picker === 'oauth'}
        onOpenChange={(open) => setPicker(open ? 'oauth' : null)}
        title={t('routes.pool.page.oauthDialogTitle')}
        description={t('routes.pool.page.oauthDialogDescription')}
        choicesClassName="grid-cols-2"
      >
        {oauthChoices.map((choice) => (
          <OAuthChoiceCard
            key={choice.agentId}
            agentId={choice.agentId}
            available={choice.available}
            label={t(oauthLabelKeys[choice.agentId])}
            unavailableLabel={t('routes.pool.page.choiceUnavailable')}
            onClick={() => selectOAuthAgent(choice.agentId)}
          />
        ))}
      </ChoiceDialog>

      <ChoiceDialog
        open={picker === 'api'}
        onOpenChange={(open) => setPicker(open ? 'api' : null)}
        title={t('routes.pool.page.apiDialogTitle')}
        description={t('routes.pool.page.apiDialogDescription')}
      >
        {apiChoices.map((choice) => (
          <ApiChoiceCard
            key={`${choice.agentId}-${choice.endpoint}`}
            available={choice.available}
            label={t(apiLabelKeys[choice.type])}
            endpoint={choice.endpoint}
            unavailableLabel={t('routes.pool.page.choiceUnavailable')}
            onClick={() => selectApiChoice(choice)}
          />
        ))}
        <button
          type="button"
          className="rounded-card border border-dashed border-border bg-panel p-3 text-left transition-colors hover:bg-hover/50"
          onClick={() => {
            setPicker(null);
            setApiDetectOpen(true);
          }}
        >
          <p className="text-sm font-medium text-primary">{t('routes.pool.page.apiDetect')}</p>
          <p className="mt-0.5 text-xs text-muted">{t('routes.pool.page.apiDetectCard')}</p>
        </button>
      </ChoiceDialog>

      <ApiDetectDialog
        open={apiDetectOpen}
        onOpenChange={setApiDetectOpen}
        onSelect={selectDetectedApiChoice}
      />

      {oauthAgentId ? (
        <OAuthFlowDialog
          agentId={oauthAgentId}
          open
          offerSwitch={false}
          poolOwned
          successDescription={t('routes.pool.page.oauthSaved')}
          onOpenChange={(open) => {
            if (open) return;
            setOauthAgentId(null);
          }}
          onStored={(account) => {
            const agentId = oauthAgentId;
            // Grok device-code completion attaches to the authorization pool
            // inside the backend operation, so a second async attach would
            // only create a misleading failure after a successful login.
            if (agentId === 'grok') {
              onChanged?.();
              return;
            }
            void attachAuthorization(
              'account',
              account.id,
              agentId,
              poolSurfaceForOAuth(agentId),
            );
          }}
          onCompleted={() => {}}
        />
      ) : null}

      {apiChoice ? (
        <ProviderEditDialog
          agentId={apiChoice.agentId}
          open
          mode="add"
          compact
          compactGrokApiBackend={apiChoice.grokApiBackend}
          compactInitialBaseUrl={apiCredentials?.baseUrl}
          compactInitialApiKey={apiCredentials?.apiKey}
          compactEndpointPresets={compactEndpointPresets}
          onOpenChange={(open) => {
            if (!open) {
              setApiChoice(null);
              setApiCredentials(null);
            }
          }}
          onSaved={(provider) => {
            const choice = apiChoice;
            setApiChoice(null);
            setApiCredentials(null);
            void attachAuthorization(
              'provider',
              provider.id,
              choice.agentId,
              poolSurfaceForApiChoice(choice),
            );
          }}
        />
      ) : null}
    </>
  );
}
