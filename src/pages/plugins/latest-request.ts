export interface LatestRequestGate {
  begin(): number;
  isCurrent(generation: number): boolean;
  invalidate(): void;
}

/** Keeps older async responses from replacing a newer inventory result. */
export function createLatestRequestGate(): LatestRequestGate {
  let current = 0;
  return {
    begin() {
      current += 1;
      return current;
    },
    isCurrent(generation) {
      return generation === current;
    },
    invalidate() {
      current += 1;
    },
  };
}

export interface ExclusiveActionGate {
  begin(): boolean;
  end(): void;
}

/** Rejects a second mutation until the current one has settled. */
export function createExclusiveActionGate(): ExclusiveActionGate {
  let active = false;
  return {
    begin() {
      if (active) return false;
      active = true;
      return true;
    },
    end() {
      active = false;
    },
  };
}
