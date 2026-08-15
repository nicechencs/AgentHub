import { Fragment } from 'react';
import { ArrowDown, ArrowRight, ArrowLeftRight, KeyRound } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { cn } from '@/lib/utils';
import type { AdapterPipelineModel, AdapterPipelineNode } from './adapter-view-model';

/**
 * Data-flow topology of the selected route: source → (local bridge?) → target.
 * A broken connector (unsupported) renders as a dashed, muted link — a neutral
 * conclusion, never a fault state.
 */
export function AdapterRoutePipeline({
  model,
  className,
}: {
  model: AdapterPipelineModel;
  className?: string;
}) {
  const showConnectorLabel = model.nodes.length === 2;
  return (
    <div
      aria-label="适配路径"
      className={cn('flex flex-col items-stretch gap-1 sm:flex-row sm:items-stretch sm:gap-0', className)}
    >
      {model.nodes.map((node, index) => (
        <Fragment key={node.kind}>
          {index > 0 && (
            <PipelineConnector
              label={showConnectorLabel ? model.connectorLabel : undefined}
              broken={model.broken}
            />
          )}
          <PipelineNode node={node} broken={model.broken} />
        </Fragment>
      ))}
    </div>
  );
}

function PipelineNode({ node, broken }: { node: AdapterPipelineNode; broken: boolean }) {
  return (
    <div
      className={cn(
        'flex min-w-0 flex-1 flex-col justify-center gap-0.5 rounded-card border px-3 py-2',
        node.kind === 'bridge' ? 'border-border bg-subtle' : 'border-border bg-panel',
        broken && node.kind === 'target' && 'border-dashed opacity-70',
      )}
    >
      <span className="flex min-w-0 items-center gap-1.5 text-sm font-medium">
        {node.kind === 'bridge' ? (
          <ArrowLeftRight className="h-3.5 w-3.5 shrink-0 text-secondary" />
        ) : node.agentId ? (
          <AgentDot agentId={node.agentId} size="sm" title={null} />
        ) : (
          <KeyRound className="h-3.5 w-3.5 shrink-0 text-secondary" />
        )}
        <span className="truncate">{node.title}</span>
      </span>
      <span className="truncate text-xs text-muted">{node.subtitle}</span>
    </div>
  );
}

function PipelineConnector({ label, broken }: { label?: string; broken: boolean }) {
  const lineClass = broken ? 'border-dashed border-border' : 'border-border-strong';
  return (
    <>
      {/* Wide: horizontal connector with optional annotation above the line. */}
      <div className="hidden shrink-0 flex-col items-center justify-center px-1 sm:flex sm:min-w-[3.5rem]">
        {label ? (
          <span className="mb-0.5 max-w-[8rem] truncate text-2xs text-muted" title={label}>{label}</span>
        ) : null}
        <span className="flex w-full items-center" aria-hidden>
          <span className={cn('h-0 flex-1 border-t', lineClass)} />
          <ArrowRight className={cn('h-3.5 w-3.5 shrink-0', broken ? 'text-muted' : 'text-secondary')} />
        </span>
      </div>
      {/* Narrow: vertical connector. */}
      <div className="flex items-center gap-1.5 py-0.5 pl-3 sm:hidden" aria-hidden>
        <ArrowDown className={cn('h-3.5 w-3.5', broken ? 'text-muted' : 'text-secondary')} />
        {label ? <span className="text-2xs text-muted">{label}</span> : null}
      </div>
    </>
  );
}
