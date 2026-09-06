import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { RuntimeRequest } from '@/lib/api/chat';
import { canSubmitRuntimeQuestions } from './chat-runtime-model';

type ReplyHandler = (request: RuntimeRequest, decision?: 'allow' | 'deny', answers?: Record<string, string[]>) => Promise<void>;

export function ChatRuntimeRequests({
  requests,
  onReply,
}: {
  requests: RuntimeRequest[];
  onReply: ReplyHandler;
}) {
  return (
    <div className="mx-auto w-full max-w-3xl space-y-2 px-4 py-2">
      {requests.map((request) => <RuntimeRequestCard key={`${request.runId}:${request.id}`} request={request} onReply={onReply} />)}
    </div>
  );
}

function RuntimeRequestCard({ request, onReply }: { request: RuntimeRequest; onReply: ReplyHandler }) {
  const { t } = useI18n();
  const [answers, setAnswers] = useState<Record<string, string[]>>({});
  const [other, setOther] = useState<Record<string, string>>({});
  const [sent, setSent] = useState(false);
  const answer = (id: string, value: string) => {
    setAnswers((current) => ({ ...current, [id]: [value] }));
    setOther((current) => ({ ...current, [id]: '' }));
  };
  const submit = async (decision?: 'allow' | 'deny') => {
    if (sent) return;
    const merged = { ...answers };
    for (const question of request.questions) if (other[question.id]?.trim()) merged[question.id] = [other[question.id].trim()];
    if (!canSubmitRuntimeQuestions(request, merged)) return;
    setSent(true);
    try { await onReply(request, decision, merged); } catch { setSent(false); }
  };
  return (
    <section className="rounded-card border border-border bg-panel p-3 text-body" aria-live="polite">
      <p className="font-medium">{request.title}</p>
      {request.detail ? <p className="mt-1 whitespace-pre-wrap text-muted">{request.detail}</p> : null}
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
      <div className="mt-3 flex gap-2">
        {request.kind === 'question' ? <Button size="sm" disabled={sent} onClick={() => submit()}>{t('chat.runtime.submit')}</Button> : <>
          <Button size="sm" disabled={sent} onClick={() => submit('allow')}>{t('chat.runtime.allow')}</Button>
          <Button size="sm" variant="outline" disabled={sent} onClick={() => submit('deny')}>{t('chat.runtime.deny')}</Button>
        </>}
      </div>
    </section>
  );
}
