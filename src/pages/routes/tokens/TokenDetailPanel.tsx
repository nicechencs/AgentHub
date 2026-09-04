import { useEffect, useRef, useState } from 'react';
import { Copy, Eye, EyeOff, Loader2, Trash2 } from 'lucide-react';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import { CopyableRouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { Hint } from '@/components/ui/tooltip';
import { Input } from '@/components/ui/input';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useToast } from '@/components/ui/toast';
import {
  listLocalTokenModels,
  testLocalToken,
} from '@/lib/api/adapter';
import type { LocalTokenProbeResult } from '@/lib/backend/contracts/adapter';
import { localEndpointBrandAgentId } from '@/lib/route-endpoints';
import { adapterStatusTextClass } from '@/pages/routes/shared/adapter-view-model';
import {
  buildTokenDetailCopyRows,
  localTokenEntryRunning,
  localTokenTestGate,
  localTokenTestInputText,
  localTokenTestModels,
  localTokenTestOutputText,
  localTokenTestResultLabel,
  localTokenTestResultTone,
  localTokenTestWindowSummary,
  tokenDetailTitle,
  tokenEndpointParts,
} from './token-detail-model';
import {
  localTokenDeleteGate,
  localTokenEditKeyGate,
  type LocalTokenRow,
} from './tokens-model';
import { TokenImportToAgentButton } from './TokenImportToAgentButton';
import type { TokenImportAgentRef } from './token-import-model';
import type { ConnectApiKeyDraft } from '@/lib/connect-flow/connect-intent';
import type { AgentKey } from '@/lib/types';

export function TokenDetailPanel({
  row,
  width,
  onClose,
  onEditKey,
  onSaveName,
  onDelete,
  installedAgents,
  onImport,
  siblingRows = [],
  writtenToNames = [],
  writtenToReady = true,
}: {
  row: LocalTokenRow;
  width?: number;
  onClose: () => void;
  onEditKey?: () => void;
  onSaveName?: (name: string) => Promise<void> | void;
  onDelete?: () => void;
  installedAgents?: readonly TokenImportAgentRef[];
  onImport?: (agentId: AgentKey, draft: ConnectApiKeyDraft) => void;
  siblingRows?: readonly LocalTokenRow[];
  writtenToNames?: readonly string[];
  writtenToReady?: boolean;
}) {
  const { t, lang } = useI18n();
  const { toast } = useToast();
  const [liveModels, setLiveModels] = useState<string[]>([]);
  const models = liveModels.length > 0 ? liveModels : localTokenTestModels(row);
  const [revealed, setRevealed] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testOpen, setTestOpen] = useState(false);
  const [testModel, setTestModel] = useState(models[0] ?? '');
  const [testResult, setTestResult] = useState<LocalTokenProbeResult | null>(null);
  const [nameDraft, setNameDraft] = useState(row.name);
  const [savingName, setSavingName] = useState(false);
  const dropdownModels = testModel.trim() && !models.includes(testModel)
    ? [testModel.trim(), ...models]
    : models;
  const rowIdRef = useRef(row.id);
  rowIdRef.current = row.id;
  const copies = buildTokenDetailCopyRows(row, revealed, t);
  const endpoint = tokenEndpointParts(row);
  const typeRow = copies.find((item) => item.id === 'type');
  const tokenRow = copies.find((item) => item.id === 'token');
  const canCopyToken = Boolean(tokenRow?.copyValue);
  const testGate = localTokenTestGate(row, t);
  const editGate = localTokenEditKeyGate(row, t);
  const deleteGate = localTokenDeleteGate(row, siblingRows, t);
  const canRunTest = testGate.enabled && Boolean(testModel.trim()) && !testing;
  const writtenTo = writtenToReady
    ? (writtenToNames.length > 0
      ? writtenToNames.join(lang === 'zh' ? '、' : ', ')
      : t('routes.tokens.writtenToNone'))
    : '—';

  useEffect(() => {
    setNameDraft(row.name);
    setSavingName(false);
    setRevealed(false);
    setTesting(false);
    setTestOpen(false);
    setTestResult(null);
    const fallback = localTokenTestModels(row);
    setLiveModels([]);
    setTestModel(fallback[0] ?? '');
    const token = row.token?.trim();
    if (!token) return;
    const requestId = row.id;
    void listLocalTokenModels(token).then((ids) => {
      if (rowIdRef.current !== requestId) return;
      const listed = ids.map((item) => item.trim()).filter(Boolean);
      if (listed.length === 0) return;
      setLiveModels(listed);
      setTestModel((current) => current.trim() || listed[0] || '');
    }).catch(() => {});
  }, [row.id, row.token]);

  useEffect(() => {
    if (testModel.trim()) return;
    setTestModel(models[0] ?? '');
  }, [models, testModel]);

  const copyToken = () => {
    const value = tokenRow?.copyValue;
    if (!value) return;
    void navigator.clipboard.writeText(value).then(
      () => toast({ title: t('routes.tokens.copied'), variant: 'success' }),
      () => toast({ title: t('routes.tokens.copyFailed'), variant: 'danger' }),
    );
  };

  const openTest = () => {
    if (!testGate.enabled || testing) return;
    setTestResult(null);
    setTestOpen(true);
  };

  const runTest = async () => {
    if (!canRunTest) return;
    const token = row.token?.trim();
    const endpointValue = row.endpoint?.trim();
    const model = testModel.trim();
    if (!token || !endpointValue || !model) return;
    const requestId = row.id;
    setTestOpen(true);
    setTestResult(null);
    setTesting(true);
    try {
      const result = await testLocalToken(endpointValue, token, row.path, model);
      if (rowIdRef.current !== requestId) return;
      setTestResult(result);
    } catch {
      if (rowIdRef.current !== requestId) return;
      setTestResult({
        outcome: 'unreachable',
        httpStatus: null,
        latencyMs: 0,
        upstreamStatus: null,
        requestUrl: null,
        requestMethod: null,
        requestBody: null,
        responseBody: null,
        errorMessage: t('routes.tokens.testFailed'),
      });
    } finally {
      if (rowIdRef.current === requestId) setTesting(false);
    }
  };

  return (
    <SideInspectPanel
      title={t('routes.tokens.detailTitle')}
      description={tokenDetailTitle(row, t)}
      onClose={onClose}
      width={width}
      headerActions={
        <>
          <Button
            variant="outline"
            size="sm"
            data-token-test=""
            disabled={!testGate.enabled || testing}
            onClick={openTest}
            title={testGate.reason ?? t('routes.tokens.test')}
            aria-label={t('routes.tokens.test')}
          >
            {testing ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
            ) : null}
            {testing ? t('routes.tokens.testing') : t('routes.tokens.test')}
          </Button>
          {installedAgents && onImport ? (
            <TokenImportToAgentButton
              row={row}
              installedAgents={installedAgents}
              onImport={onImport}
            />
          ) : null}
          {onDelete ? (
            <Button
              variant="dangerOutline"
              size="sm"
              data-token-delete=""
              disabled={!deleteGate.enabled}
              onClick={() => {
                if (!deleteGate.enabled) return;
                onDelete();
              }}
              title={deleteGate.reason ?? t('routes.tokens.delete')}
              aria-label={t('routes.tokens.delete')}
            >
              <Trash2 className="h-3.5 w-3.5" aria-hidden />
              {t('routes.tokens.delete')}
            </Button>
          ) : null}
          {onEditKey ? (
            <Button
              variant="outline"
              size="sm"
              data-token-edit-key=""
              disabled={!editGate.enabled}
              onClick={() => {
                if (!editGate.enabled) return;
                onEditKey();
              }}
              title={editGate.reason ?? t('routes.tokens.editKey')}
              aria-label={t('routes.tokens.editKey')}
            >
              {t('routes.tokens.editKey')}
            </Button>
          ) : null}
        </>
      }
    >
      <div className="flex flex-col gap-3 text-sm" data-token-detail={row.id}>
        <div className="space-y-1">
          <p className="text-meta text-muted">{t('routes.tokens.fieldName')}</p>
          <Input
            className="min-w-0 w-full"
            value={nameDraft}
            onChange={(event) => setNameDraft(event.target.value)}
            placeholder={t('routes.tokens.namePlaceholder')}
            disabled={savingName || !onSaveName || row.unavailable}
            aria-label={t('routes.tokens.fieldName')}
          />
          {onSaveName ? (
            <div className="flex flex-wrap items-center gap-1.5">
              <Button
                variant="outline"
                size="sm"
                disabled={savingName || row.unavailable || !nameDraft.trim()}
                onClick={() => {
                  if (savingName) return;
                  setSavingName(true);
                  void Promise.resolve(onSaveName(nameDraft)).finally(() => {
                    if (rowIdRef.current === row.id) setSavingName(false);
                  });
                }}
              >
                {t('routes.tokens.saveName')}
              </Button>
            </div>
          ) : null}
        </div>
        <div className="space-y-1">
          <p className="text-meta text-muted">{t('routes.tokens.fieldToken')}</p>
          <div className="flex min-w-0 items-center gap-1">
            <p className="min-w-0 flex-1 break-all font-mono text-secondary">
              {tokenRow?.display}
            </p>
            {canCopyToken ? (
              <>
                <Hint label={revealed ? t('common.hideSecret') : t('common.showSecret')}>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="h-7 w-7 shrink-0 px-0"
                    data-token-reveal=""
                    onClick={() => setRevealed((current) => !current)}
                    aria-label={revealed ? t('common.hideSecret') : t('common.showSecret')}
                  >
                    {revealed ? <EyeOff className="h-3 w-3" aria-hidden /> : <Eye className="h-3 w-3" aria-hidden />}
                  </Button>
                </Hint>
                <Hint label={t('routes.tokens.copy')}>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="h-7 w-7 shrink-0 px-0"
                    data-token-copy=""
                    onClick={copyToken}
                    aria-label={t('routes.tokens.copy')}
                  >
                    <Copy className="h-3 w-3" aria-hidden />
                  </Button>
                </Hint>
              </>
            ) : null}
          </div>
          {testResult ? (
            <p
              className={adapterStatusTextClass(localTokenTestResultTone(testResult.outcome))}
              data-token-test-result={testResult.outcome}
            >
              {localTokenTestResultLabel(testResult, t)}
            </p>
          ) : testGate.reason ? (
            <p className="text-meta text-muted">{testGate.reason}</p>
          ) : editGate.reason ? (
            <p className="text-meta text-muted" data-token-edit-key-hint="">{editGate.reason}</p>
          ) : null}
        </div>
        <div className="min-w-0 space-y-1">
          <p className="text-meta text-muted">{t('routes.tokens.fieldEndpoint')}</p>
          {endpoint.portPending ? (
            <p className="font-mono text-muted">{t('routes.pendingPort')}</p>
          ) : (
            <CopyableRouteEndpointUrl
              path={row.path}
              port={endpoint.portPending ? null : Number(endpoint.portLabel)}
              host={endpoint.host}
              endpointId={endpoint.endpointId}
              brandAgentId={localEndpointBrandAgentId(row.kind)}
              className="text-sm"
            />
          )}
        </div>
        <div className="space-y-1">
          <p className="text-meta text-muted">{t('routes.tokens.fieldType')}</p>
          <p className="text-primary">{typeRow?.display}</p>
        </div>
        <div className="space-y-1" data-token-written-to="">
          <p className="text-meta text-muted">{t('routes.tokens.fieldWrittenTo')}</p>
          <p className="text-secondary">{writtenTo}</p>
        </div>
      </div>
      <Dialog open={testOpen} onOpenChange={(open) => { if (!testing) setTestOpen(open); }}>
        <DialogContent data-token-test-window="">
          <DialogHeader>
            <DialogTitle>{t('routes.tokens.testWindowTitle')}</DialogTitle>
            <DialogDescription>
              {localTokenTestWindowSummary(testResult, testing, t)}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3 text-sm">
            <div className="space-y-1">
              <p className="text-meta text-muted">{t('routes.tokens.testModel')}</p>
              {dropdownModels.length > 0 ? (
                <Select
                  value={testModel.trim() ? testModel : undefined}
                  onValueChange={setTestModel}
                  disabled={testing}
                >
                  <SelectTrigger
                    className="w-full"
                    data-token-test-model=""
                    aria-label={t('routes.tokens.testModel')}
                  >
                    <SelectValue placeholder={t('routes.tokens.testNeedModel')} />
                  </SelectTrigger>
                  <SelectContent>
                    {dropdownModels.map((model) => (
                      <SelectItem key={model} value={model}>
                        {model}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              ) : (
                <p className="text-secondary" data-token-test-no-model="">
                  {t('routes.tokens.testNoModels')}
                </p>
              )}
              <Input
                value={testModel}
                onChange={(event) => setTestModel(event.target.value)}
                disabled={testing}
                placeholder={t('routes.tokens.testModelCustom')}
                data-token-test-model-custom=""
                aria-label={t('routes.tokens.testModelCustom')}
              />
            </div>
            <div className="space-y-1">
              <p className="text-meta text-muted">{t('routes.tokens.testInput')}</p>
              <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-all rounded-card border border-border bg-hover px-3 py-2 font-mono text-meta text-secondary">
                {localTokenTestInputText(row, testResult)}
              </pre>
            </div>
            <div className="space-y-1">
              <p className="text-meta text-muted">{t('routes.tokens.testOutput')}</p>
              <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all rounded-card border border-border bg-hover px-3 py-2 font-mono text-meta text-secondary">
                {localTokenTestOutputText(testResult, {
                  running: localTokenEntryRunning(row),
                  testing,
                }, t)}
              </pre>
            </div>
          </div>
          <DialogFooter>
            <Button
              data-token-test-run=""
              disabled={!canRunTest}
              onClick={() => { void runTest(); }}
            >
              {testing ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
              ) : null}
              {testing ? t('routes.tokens.testing') : t('routes.tokens.test')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </SideInspectPanel>
  );
}
