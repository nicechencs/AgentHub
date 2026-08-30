import { foldEffect, foldable, syntaxTree } from '@codemirror/language';

const CONTAINER_NODES = new Set(['Object', 'Array']);

type JsonPreviewView = {
  state: Parameters<typeof syntaxTree>[0];
  dispatch: (spec: { effects: ReturnType<typeof foldEffect.of>[] }) => void;
};

/** Collapse objects/arrays deeper than `maxDepth` (root object is depth 1). */
export function foldJsonBeyondDepth(view: JsonPreviewView, maxDepth = 2): void {
  const { state } = view;
  if (state.doc.lines < 12) return;

  const effects: ReturnType<typeof foldEffect.of>[] = [];
  let depth = 0;
  syntaxTree(state).iterate({
    enter(node) {
      if (!CONTAINER_NODES.has(node.name)) return;
      depth += 1;
      if (depth <= maxDepth) return;
      const range = foldable(state, node.from, node.to);
      if (range) effects.push(foldEffect.of(range));
      return false;
    },
    leave(node) {
      if (CONTAINER_NODES.has(node.name)) depth -= 1;
    },
  });
  if (effects.length > 0) view.dispatch({ effects });
}
