import { useEffect, useRef, useState } from 'react';
import { Loader2 } from 'lucide-react';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import { CopyableRouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
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
import { listLocalTokenModels, setLocalTokenCustomModels, testLocalToken } from '@/lib/api/adapter';
import type { LocalTokenProbeResult } from '@/lib/backend/contracts/adapter';
import { localEndpointBrandAgentId } from '@/lib/route-endpoints';
import { adapterStatusTextClass } from '@/pages/bridges/adapter-view-model';
import {
  buildTokenDetailCopyRows,
  formatTokenRelative,
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
  tokenLastPageDisplay,
  tokenUsageDisplay,
} from './token-detail-model';
import type { LocalTokenRow } from './tokens-model';
import { parseCustomModelList } from '@/pages/routes/pool/pool-authorization-detail';

export function TokenDetailPanel({
  row,
  width,
  onClose,
  onEditKey,
}: {
  row: LocalTokenRow;
  width?: number;
  onClose: () => void;
  onEditKey?: () => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [liveModels, setLiveModels] = useState<string[]>([]);
  const models = liveModels.length > 0 ? liveModels : localTokenTestModels(row);
  const [revealed, setRevealed] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testOpen, setTestOpen] = useState(false);
  const [testModel, setTestModel] = useState(models[0] ?? '');
  const [testResult, setTestResult] = useState<LocalTokenProbeResult | null>(null);
  const [modelDraft, setModelDraft] = useState('');
  const [savingModels, setSavingModels] = useState(false);
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
  const canRunTest = testGate.enabled && Boolean(testModel.trim()) && !testing;

  useEffect(() => {
    setRevealed(false);
    setTesting(false);
    setTestOpen(false);
    setTestResult(null);
    setLiveModels([]);
    setTestModel(localTokenTestModels(row)[0] ?? '');
    setModelDraft(localTokenTestModels(row).join('\n'));
  }, [row.id]);

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
    const token = row.token?.trim();
    if (!token) return;
    const requestId = row.id;
    void listLocalTokenModels(token).then((ids) => {
      if (rowIdRef.current !== requestId) return;
      const listed = ids.map((item) => item.trim()).filter(Boolean);
      if (listed.length === 0) return;
      setLiveModels(listed);
      setModelDraft(listed.join('\n'));
      setTestModel((current) => current.trim() || listed[0] || '');
    }).catch(() => {});
  };

  const saveCustomModels = async () => {
    const token = row.token?.trim();
    if (!token) return;
    const listed = parseCustomModelList(modelDraft);
    setSavingModels(true);
    try {
      const next = await setLocalTokenCustomModels(token, listed);
      setLiveModels(next);
      setModelDraft(next.join('\n'));
      setTestModel((current) => current.trim() || next[0] || '');
      toast({ title: t('common.save'), variant: 'success' });
    } catch {
      toast({ title: t('common.saveFailed'), variant: 'danger' });
    } finally {
      setSavingModels(false);
    }
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
    >
      <div className="flex flex-col gap-3 text-sm" data-token-detail={row.id}>
        <div className="space-y-1">
          <p className="text-meta text-muted">{t('routes.tokens.fieldType')}</p>
          <p className="text-primary">{typeRow?.display}</p>
        </div>
        <div className="space-y-1">
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
          <p className="text-meta text-muted">{t('routes.tokens.fieldLastPage')}</p>
          <p className="font-mono text-secondary">{tokenLastPageDisplay(row) || '—'}</p>
          {row.lastRequestAt ? (
            <p className="text-meta text-muted">
              {t('routes.tokens.fieldLastAt')} · {formatTokenRelative(row.lastRequestAt, t)}
            </p>
          ) : null}
        </div>
        <div className="space-y-1">
          <p className="text-meta text-muted">{t('routes.tokens.fieldUsage')}</p>
          <p className="text-secondary">{tokenUsageDisplay(row.usage, t) || '—'}</p>
        </div>
        <div className="space-y-1">
          <p className="text-meta text-muted">{t('routes.tokens.fieldToken')}</p>
          <p className="min-w-0 break-all font-mono text-secondary">
            {tokenRow?.display}
          </p>
          <div className="flex flex-wrap items-center gap-1.5">
            {canCopyToken ? (
              <>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setRevealed((current) => !current)}
                >
                  {revealed ? t('common.hideSecret') : t('common.showSecret')}
                </Button>
                <Button variant="outline" size="sm" onClick={copyToken}>
                  {t('routes.tokens.copy')}
                </Button>
              </>
            ) : null}
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
            {onEditKey ? (
              <Button variant="outline" size="sm" onClick={onEditKey}>
                {t('routes.tokens.editKey')}
              </Button>
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
          ) : null}
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
              <p className="text-meta text-secondary">{t('routes.tokens.testModelsHint')}</p>
              <textarea
                value={modelDraft}
                onChange={(event) => setModelDraft(event.target.value)}
                disabled={testing || savingModels}
                placeholder={t('routes.tokens.testModelsPlaceholder')}
                rows={4}
                className="min-h-[5.5rem] w-full resize-y rounded-card border border-border bg-transparent px-3 py-2 font-mono text-xs text-primary"
                data-token-test-models-draft=""
              />
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={testing || savingModels || !row.token?.trim()}
                onClick={() => { void saveCustomModels(); }}
              >
                {savingModels ? t('common.saving') : t('routes.tokens.testModelsSave')}
              </Button>
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
