import { useEffect, useMemo, useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName } from '@/config/agents';
import {
  listMcpCatalog,
  probeMcpServer,
  upsertMcpServer,
} from '@/lib/api/mcp';
import type { McpCatalogEntry, McpServerSpec } from '@/lib/backend/contracts/mcp-types';
import type { AgentKey } from '@/lib/types';
import { MCP_WRITE_AGENTS } from './writable-agents';

function FieldLabel({ children }: { children: React.ReactNode }) {
  return <div className="text-meta font-medium text-secondary">{children}</div>;
}

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  defaultAgent?: AgentKey | 'all';
  onWritten: () => void;
};

export function McpWriteDialog({ open, onOpenChange, defaultAgent, onWritten }: Props) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [catalog, setCatalog] = useState<McpCatalogEntry[]>([]);
  const [agent, setAgent] = useState<AgentKey>('claude');
  const [templateId, setTemplateId] = useState('custom');
  const [name, setName] = useState('memory');
  const [command, setCommand] = useState('npx');
  const [argsText, setArgsText] = useState('-y @modelcontextprotocol/server-memory');
  const [url, setUrl] = useState('');
  const [transport, setTransport] = useState('stdio');
  const [busy, setBusy] = useState(false);
  const [probeNote, setProbeNote] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    const preferred =
      defaultAgent && defaultAgent !== 'all' && (MCP_WRITE_AGENTS as readonly string[]).includes(defaultAgent)
        ? defaultAgent
        : 'claude';
    setAgent(preferred);
    setProbeNote(null);
    if (preferred === 'codex') setTransport('stdio');
    void listMcpCatalog()
      .then(setCatalog)
      .catch(() => setCatalog([]));
  }, [open, defaultAgent]);

  const selectedTemplate = useMemo(
    () => catalog.find((row) => row.id === templateId) ?? null,
    [catalog, templateId],
  );

  useEffect(() => {
    if (!selectedTemplate) return;
    setName(selectedTemplate.name);
    setTransport(selectedTemplate.transport || 'stdio');
    setCommand(selectedTemplate.command ?? '');
    setArgsText((selectedTemplate.args ?? []).join(' '));
    setUrl(selectedTemplate.url ?? '');
  }, [selectedTemplate]);

  function buildSpec(): McpServerSpec {
    const args = argsText
      .split(/\s+/)
      .map((s) => s.trim())
      .filter(Boolean);
    return {
      name: name.trim(),
      transport,
      command: transport === 'stdio' ? command.trim() || null : null,
      args: transport === 'stdio' ? args : [],
      url: transport === 'stdio' ? null : url.trim() || null,
      enabled: true,
    };
  }

  async function onProbe() {
    setBusy(true);
    setProbeNote(null);
    try {
      const result = await probeMcpServer(buildSpec());
      setProbeNote(result.message);
      toast({
        title: result.ok ? t('mcp.write.probeOk') : t('mcp.write.probeFail'),
        description: result.message,
        variant: result.ok ? 'default' : 'danger',
      });
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setProbeNote(message);
      toast({ title: t('mcp.write.probeFail'), description: message, variant: 'danger' });
    } finally {
      setBusy(false);
    }
  }

  async function onWrite() {
    setBusy(true);
    try {
      const result = await upsertMcpServer(agent, buildSpec());
      toast({
        title: t('mcp.write.saved'),
        description: t('mcp.write.savedHint', {
          agent: agentDisplayName(result.agent),
          name: result.name,
        }),
      });
      onOpenChange(false);
      onWritten();
    } catch (e) {
      toast({
        title: t('mcp.write.saveFail'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('mcp.write.title')}</DialogTitle>
          <DialogDescription>{t('mcp.write.description')}</DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          <div className="space-y-1.5">
            <FieldLabel>{t('mcp.write.agent')}</FieldLabel>
            <Select
              value={agent}
              onValueChange={(v) => {
                const next = v as AgentKey;
                setAgent(next);
                if (next === 'codex') setTransport('stdio');
              }}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {MCP_WRITE_AGENTS.map((id) => (
                  <SelectItem key={id} value={id}>
                    {agentDisplayName(id)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-1.5">
            <FieldLabel>{t('mcp.write.template')}</FieldLabel>
            <Select value={templateId} onValueChange={setTemplateId}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="custom">{t('mcp.write.custom')}</SelectItem>
                {catalog.map((row) => (
                  <SelectItem key={row.id} value={row.id}>
                    {row.title}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {selectedTemplate ? (
              <p className="text-meta text-secondary">{selectedTemplate.description}</p>
            ) : null}
          </div>

          <div className="space-y-1.5">
            <FieldLabel>{t('mcp.write.name')}</FieldLabel>
            <Input value={name} onChange={(e) => setName(e.target.value)} />
          </div>

          <div className="space-y-1.5">
            <FieldLabel>{t('mcp.write.transport')}</FieldLabel>
            <Select
              value={transport}
              onValueChange={setTransport}
              disabled={agent === 'codex'}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="stdio">stdio</SelectItem>
                <SelectItem value="http">HTTP</SelectItem>
                <SelectItem value="sse">SSE</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {transport === 'stdio' ? (
            <>
              <div className="space-y-1.5">
                <FieldLabel>{t('mcp.write.command')}</FieldLabel>
                <Input value={command} onChange={(e) => setCommand(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <FieldLabel>{t('mcp.write.args')}</FieldLabel>
                <Input value={argsText} onChange={(e) => setArgsText(e.target.value)} />
              </div>
            </>
          ) : (
            <div className="space-y-1.5">
              <FieldLabel>{t('mcp.write.url')}</FieldLabel>
              <Input value={url} onChange={(e) => setUrl(e.target.value)} />
            </div>
          )}

          {probeNote ? <p className="text-meta text-secondary">{probeNote}</p> : null}
        </div>

        <DialogFooter className="gap-2 sm:gap-2">
          <Button type="button" variant="outline" disabled={busy} onClick={() => void onProbe()}>
            {t('mcp.write.probe')}
          </Button>
          <Button type="button" disabled={busy || !name.trim()} onClick={() => void onWrite()}>
            {t('mcp.write.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
