import { useEffect, useState, type ReactNode } from 'react';
import { AgentThinking } from '@/components/shared/AgentThinking';
import { useI18n } from '@/components/shared/LanguageProvider';
import {
  classifyToolAction,
  formatToolStep,
  latestThinkingStep,
  thinkingElapsedMs,
  toolActionTone,
  transcriptTimelineSteps,
  type AgentProcessView,
} from '@/lib/chat-process';
import type { AgentKey, ProcessStep } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  extractStepEditFiles,
  formatTurnEditRowDetail,
  sameEditPath,
  type TurnEditFile,
} from './chat-edit-preview';
import { ChatExpandAffordance } from './ChatExpandAffordance';
import { thinkingChromeLabel } from './chat-format';
import {
  PROCESS_INSPECT_GENERATING_KEY,
  processInspectRowAction,
  processInspectStepKey,
} from './chat-preview-model';

export function ChatTurnProcessList({
  process,
  turn,
  agent,
  running,
  processPaneOpen = false,
  selectedStepKey = null,
  selectedEditPath = '',
  selectedEditTurn,
  selectedEditStepId,
  onOpenProcess,
  onCloseProcess,
  onSelectEdit,
}: {
  process?: AgentProcessView;
  turn: number;
  agent: AgentKey;
  running: boolean;
  processPaneOpen?: boolean;
  /** Step currently shown in the detail pane. Only set while that pane is open. */
  selectedStepKey?: string | null;
  selectedEditPath?: string;
  selectedEditTurn?: number;
  selectedEditStepId?: string;
  onOpenProcess?: (turn: number, agent: AgentKey, stepKey: string) => void;
  onCloseProcess?: () => void;
  onSelectEdit?: (file: TurnEditFile, turn: number) => void;
}) {
  const { t } = useI18n();
  const timeline = transcriptTimelineSteps(process?.steps);
  const latestThinking = latestThinkingStep(timeline);
  const liveThinking = Boolean(latestThinking && !latestThinking.done);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!liveThinking) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [liveThinking]);

  const openProcess = (stepKey: string) => {
    if (!onOpenProcess) return;
    if (processInspectRowAction(processPaneOpen, selectedStepKey, stepKey) === 'close') {
      onCloseProcess?.();
      return;
    }
    onOpenProcess(turn, agent, stepKey);
  };

  if (timeline.length === 0) {
    if (!running || !onOpenProcess) return null;
    return (
      <ol className="mb-1 space-y-0.5" data-help="chat-turn-process">
        <li>
          <ProcessRowButton
            help="chat-process-chip"
            expanded={processPaneOpen && selectedStepKey === PROCESS_INSPECT_GENERATING_KEY}
            live
            onClick={() => openProcess(PROCESS_INSPECT_GENERATING_KEY)}
          >
            {t('chat.process.summaryGenerating')}
          </ProcessRowButton>
        </li>
      </ol>
    );
  }

  return (
    <ol className="mb-1 space-y-0.5" data-help="chat-turn-process">
      {timeline.map((step, index) => {
        const key = processRowKey(step, index);
        const stepKey = processInspectStepKey(index);
        const stepOpen = processPaneOpen && selectedStepKey === stepKey;
        if (step.type === 'thinking') {
          const latest = step === latestThinking;
          const done = Boolean(step.done);
          const elapsed = latest && process ? thinkingElapsedMs(process, now) : 0;
          const label = thinkingChromeLabel(done, elapsed, t);
          return (
            <li key={key}>
              <ProcessRowButton
                help="chat-thinking-bar"
                expanded={stepOpen}
                current={stepOpen}
                live={!done}
                disabled={!onOpenProcess}
                onClick={() => openProcess(stepKey)}
              >
                {done ? (
                  <span className="min-w-0 truncate">{label}</span>
                ) : (
                  <AgentThinking label={label} showTimer={false} />
                )}
              </ProcessRowButton>
            </li>
          );
        }
        if (step.type === 'tool') {
          const live = toolActionTone(step.status) === 'live';
          const stepFiles = editFilesForStep(step, process?.steps ?? []);
          const rows = stepFiles.length > 0 ? stepFiles : [null];
          return rows.map((editFile, fileIndex) => {
            const selected = Boolean(
              editFile
              && selectedEditPath
              && sameEditPath(selectedEditPath, editFile.path)
              && (typeof selectedEditTurn !== 'number' || selectedEditTurn === turn)
              && (!selectedEditStepId || selectedEditStepId === editFile.stepId),
            );
            const openEdit = Boolean(editFile && onSelectEdit);
            const baseLabel = editFile && stepFiles.length > 1
              ? formatToolStep({ ...step, input: { path: editFile.path } }, t)
              : formatToolStep(step, t);
            const detail = editFile ? formatTurnEditRowDetail(editFile) : '';
            const label = `${baseLabel}${detail}`;
            return (
              <li key={stepFiles.length > 1 ? `${key}:${fileIndex}:${editFile?.path}` : key}>
                <ProcessRowButton
                  help="chat-process-chip"
                  expanded={openEdit ? selected : stepOpen}
                  live={live}
                  current={openEdit ? selected : stepOpen}
                  disabled={!onOpenProcess && !openEdit}
                  onClick={() => {
                    if (openEdit && editFile) onSelectEdit?.(editFile, turn);
                    else openProcess(stepKey);
                  }}
                >
                  <span className="min-w-0 truncate">{label}</span>
                </ProcessRowButton>
              </li>
            );
          });
        }
        if (step.type === 'error') {
          return (
            <li key={key}>
              <ProcessRowButton
                help="chat-process-chip"
                expanded={stepOpen}
                current={stepOpen}
                onClick={() => openProcess(stepKey)}
                disabled={!onOpenProcess}
              >
                <span className="min-w-0 truncate">{step.message}</span>
              </ProcessRowButton>
            </li>
          );
        }
        return null;
      })}
    </ol>
  );
}

function editFilesForStep(
  step: Extract<ProcessStep, { type: 'tool' }>,
  steps: ProcessStep[],
): TurnEditFile[] {
  if (classifyToolAction(step.name) !== 'edit') return [];
  const index = steps.indexOf(step);
  return extractStepEditFiles(step, index >= 0 ? index : 0);
}

function processRowKey(step: ProcessStep, index: number): string {
  if (step.type === 'tool') return `tool:${step.id ?? step.name}:${index}`;
  if (step.type === 'thinking') return `thinking:${index}`;
  if (step.type === 'error') return `error:${index}`;
  return `${step.type}:${index}`;
}

function ProcessRowButton({
  help,
  expanded,
  live = false,
  current = false,
  disabled = false,
  onClick,
  children,
}: {
  help: string;
  expanded: boolean;
  live?: boolean;
  current?: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  const { t } = useI18n();
  const hint = expanded ? t('chat.runtime.collapseRow') : t('chat.runtime.expandRow');
  return (
    <button
      type="button"
      className={cn(
        'group inline-flex max-w-full items-center gap-1.5 rounded-btn px-1 py-0.5 text-left text-meta text-secondary hover:bg-hover hover:text-primary',
        live && 'agent-progress-running font-medium text-primary',
        current && 'bg-hover',
      )}
      data-help={help}
      aria-expanded={expanded}
      aria-current={current ? 'true' : undefined}
      disabled={disabled}
      onClick={onClick}
    >
      <ChatExpandAffordance expanded={expanded} label={hint} />
      {children}
    </button>
  );
}
