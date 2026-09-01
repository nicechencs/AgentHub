import { useEffect, useRef, useState } from 'react';
import { Loader2 } from 'lucide-react';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import { CopyableRouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useToast } from '@/components/ui/toast';
import { testLocalToken } from '@/lib/api/adapter';
import type { LocalTokenProbeResult } from '@/lib/backend/contracts/adapter';
import { localEndpointBrandAgentId } from '@/lib/route-endpoints';
import { adapterStatusTextClass } from '@/pages/bridges/adapter-view-model';
import {
  buildTokenDetailCopyRows,
  formatTokenRelative,
  localTokenEntryRunning,
  localTokenTestDurationLabel,
  localTokenTestGate,
  localTokenTestInputText,
  localTokenTestOutputText,
  localTokenTestResultLabel,
  localTokenTestResultTone,
  tokenDetailTitle,
  tokenEndpointParts,
  tokenLastPageDisplay,
  tokenUsageDisplay,
} from './token-detail-model';
import type { LocalTokenRow } from './tokens-model';

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
  const [revealed, setRevealed] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testOpen, setTestOpen] = useState(false);
  const [testResult, setTestResult] = useState<LocalTokenProbeResult | null>(null);
  const rowIdRef = useRef(row.id);
  rowIdRef.current = row.id;
  const copies = buildTokenDetailCopyRows(row, revealed, t);
  const endpoint = tokenEndpointParts(row);
  const typeRow = copies.find((item) => item.id === 'type');
  const tokenRow = copies.find((item) => item.id === 'token');
  const canCopyToken = Boolean(tokenRow?.copyValue);
  const testGate = localTokenTestGate(row, t);

  useEffect(() => {
    setRevealed(false);
    setTesting(false);
    setTestOpen(false);
    setTestResult(null);
  }, [row.id]);

  const copyToken = () => {
    const value = tokenRow?.copyValue;
    if (!value) return;
    void navigator.clipboard.writeText(value).then(
      () => toast({ title: t('routes.tokens.copied'), variant: 'success' }),
      () => toast({ title: t('routes.tokens.copyFailed'), variant: 'danger' }),
    );
  };

  const runTest = async () => {
    if (!testGate.enabled || testing) return;
    const token = row.token?.trim();
    const endpointValue = row.endpoint?.trim();
    if (!token || !endpointValue) return;
    const requestId = row.id;
    setTestOpen(true);
    setTestResult(null);
    setTesting(true);
    try {
      const result = await testLocalToken(endpointValue, token);
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
              onClick={() => { void runTest(); }}
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
              {testing
                ? t('routes.tokens.testing')
                : testResult
                  ? localTokenTestResultLabel(testResult, t)
                  : t('routes.tokens.testNoOutput')}
              {!testing && testResult
                ? ` · ${t('routes.tokens.testDuration')} ${localTokenTestDurationLabel(testResult.latencyMs, t)}`
                : null}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3 text-sm">
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
        </DialogContent>
      </Dialog>
    </SideInspectPanel>
  );
}
