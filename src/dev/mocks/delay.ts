/** Mock / test latency helpers — not part of production contracts. */
export const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
export const randomLatency = (min = 200, span = 400) => min + Math.random() * span;
