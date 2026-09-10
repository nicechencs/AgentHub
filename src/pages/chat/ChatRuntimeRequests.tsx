import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Tip } from '@/components/ui/tooltip';
import type { RuntimeDecision, RuntimeRequest } from '@/lib/api/chat';
import { cn } from '@/lib/utils';
import {
  canSubmitRuntimeQuestions,
  fileChangeKindLabel,
  fileChangePreviewHintKey,
  runtimeAllowAlwaysCopy,
  runtimeFileChangePreview,
  runtimeRequestTitle,
} from './chat-runtime-model';

type ReplyHandler = (request: RuntimeRequest, decision?: RuntimeDecision, answers?: Record<string, string[]>) => Promise<void>;

export function ChatRuntimeRequests({
  requests,
  onReply,
  agentId,
}: {
  requests: RuntimeRequest[];
  onReply: ReplyHandler;
  agentId?: string | null;
}) {
  return (
    <div className="w-full space-y-2 py-2">
      {requests.map((request) => (
        <RuntimeRequestCard
          key={`${request.runId}:${request.id}`}
          request={request}
          onReply={onReply}
          agentId={agentId}
        />
      ))}
    </div>
  );
}

function RuntimeRequestCard({
  request,
  onReply,
  agentId,
}: {
  request: RuntimeRequest;
  onReply: ReplyHandler;
  agentId?: string | null;
}) {
  const { t } = useI18n();
  const [answers, setAnswers] = useState<Record<string, string[]>>({});
  const [other, setOther] = useState<Record<string, string>>({});
  const [sent, setSent] = useState(false);
  const answer = (id: string, value: string) => {
    setAnswers((current) => ({ ...current, [id]: [value] }));
    setOther((current) => ({ ...current, [id]: '' }));
  };
  const submit = async (decision?: RuntimeDecision) => {
    if (sent) return;
    const merged = { ...answers };
    for (const question of request.questions) if (other[question.id]?.trim()) merged[question.id] = [other[question.id].trim()];
    if (!canSubmitRuntimeQuestions(request, merged)) return;
    setSent(true);
    try {
      await onReply(request, decision, request.kind === 'question' ? merged : undefined);
    } catch { setSent(false); }
  };
  const title = runtimeRequestTitle(t, request);
  const always = runtimeAllowAlwaysCopy({ request, agentId });
  return (
    <section className="rounded-card border border-border bg-subtle p-3 text-body" aria-live="polite">
      <p className="font-medium text-primary">{title}</p>
      {showRequestDetail(request) ? (
        <p className={cn(
          'mt-1 whitespace-pre-wrap text-meta text-secondary',
          request.kind === 'file' && 'font-mono',
        )}>{request.detail}</p>
      ) : null}
      <FileChangePreview request={request} />
      {request.kind === 'question' ? request.questions.map((question) => (
        <fieldset key={question.id} className="mt-3 space-y-1.5">
          <legend className="font-medium">{question.header || question.question}</legend>
          {question.header && question.question ? <p className="text-meta text-muted">{question.question}</p> : null}
          {question.options.map((option) => (
            <label key={option.label} className="flex cursor-pointer gap-2 rounded px-1 py-1 hover:bg-subtle">
              <input type="radio" name={`${request.id}:${question.id}`} disabled={sent} checked={answers[question.id]?.[0] === option.label} onChange={() => answer(question.id, option.label)} />
              <span><span>{option.label}</span>{option.description ? <span className="block text-meta text-muted">{option.description}</span> : null}</span>
            </label>
          ))}
          {(question.isOther || question.options.length === 0) ? <input type={question.isSecret ? 'password' : 'text'} className="w-full rounded border border-border bg-canvas px-2 py-1" disabled={sent} value={other[question.id] ?? ''} onChange={(event) => { setOther((current) => ({ ...current, [question.id]: event.target.value })); setAnswers((current) => ({ ...current, [question.id]: [] })); }} aria-label={question.question} /> : null}
        </fieldset>
      )) : null}
      <div className="mt-3 flex flex-wrap items-center gap-2">
        {request.kind === 'question' ? <Button size="sm" disabled={sent} onClick={() => submit()}>{t('chat.runtime.submit')}</Button> : <>
          <Button size="sm" disabled={sent} onClick={() => submit('allow')}>{t('chat.runtime.allow')}</Button>
          {always.shown ? (
            <span className="inline-flex flex-wrap items-center gap-2" data-help="chat-allow-always">
              <Button size="sm" variant="outline" disabled={sent} onClick={() => submit('allow_always')}>
                {t('chat.runtime.allowAlways')}
              </Button>
              <span className="text-meta text-muted">{t(always.hintKey)}</span>
            </span>
          ) : null}
          <Button size="sm" variant="ghost" disabled={sent} onClick={() => submit('deny')}>{t('chat.runtime.deny')}</Button>
        </>}
      </div>
    </section>
  );
}

function showRequestDetail(request: RuntimeRequest): boolean {
  if (!request.detail) return false;
  const preview = runtimeFileChangePreview(request);
  return !preview.shown || (preview.empty && preview.rows.length === 0);
}

function FileChangePreview({ request }: { request: RuntimeRequest }) {
  const { t } = useI18n();
  const preview = runtimeFileChangePreview(request);
  if (!preview.shown) return null;
  const hintKey = fileChangePreviewHintKey(preview);
  return (
    <div
      className="mt-2 space-y-2"
      data-help={
        preview.empty
          ? hintKey === 'chat.runtime.fileChangePathOnly'
            ? 'chat-file-change-preview-path-only'
            : 'chat-file-change-preview-empty'
          : 'chat-file-change-preview'
      }
    >
      {preview.rows.map((row, index) => (
        <div key={`${row.path}:${index}`} className="space-y-1">
          <p className="flex min-w-0 items-baseline gap-2 font-mono text-meta text-secondary">
            {row.kind ? (
              <span className="shrink-0 font-sans text-muted">{fileChangeKindLabel(row.kind, t)}</span>
            ) : null}
            <span className="min-w-0 flex-1">
              <Tip
                label={row.path || t('chat.runtime.fileChangePathMissing')}
                className="block truncate"
              >
                {row.path || t('chat.runtime.fileChangePathMissing')}
              </Tip>
            </span>
          </p>
          {!preview.empty && row.preview ? (
            <pre className="max-h-36 overflow-auto whitespace-pre-wrap break-all rounded-card border border-border/60 bg-canvas px-2 py-1.5 font-mono text-meta leading-relaxed text-primary">
              {row.preview}
            </pre>
          ) : null}
        </div>
      ))}
      {hintKey ? (
        <p className="text-meta text-muted">{t(hintKey)}</p>
      ) : null}
    </div>
  );
}
