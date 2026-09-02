import { Component, type ErrorInfo, type ReactNode } from 'react';
import { ErrorState } from '@/components/shared/ErrorState';

type Props = {
  children: ReactNode;
};

type State = {
  error: Error | null;
  /** One silent remount for cold-start / hard-refresh DOM races. */
  autoRetried: boolean;
};

/**
 * Catches render-tree failures so one page crash does not blank the whole app.
 * Distinguishes a brief recover-and-retry from a lasting failure (UX-11).
 *
 * Recovering UI must stay free of i18n/providers — this boundary wraps Root.
 */
export class AppErrorBoundary extends Component<Props, State> {
  state: State = { error: null, autoRetried: false };
  private recoverTimer: number | null = null;

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('AppErrorBoundary', error, info.componentStack);
    if (this.state.autoRetried) return;
    if (this.recoverTimer != null) return;
    this.recoverTimer = window.setTimeout(() => {
      this.recoverTimer = null;
      this.setState({ error: null, autoRetried: true });
    }, 600);
  }

  componentWillUnmount(): void {
    if (this.recoverTimer != null) {
      window.clearTimeout(this.recoverTimer);
    }
  }

  private retry = () => {
    if (this.recoverTimer != null) {
      window.clearTimeout(this.recoverTimer);
      this.recoverTimer = null;
    }
    this.setState({ error: null });
  };

  render() {
    if (this.state.error && !this.state.autoRetried) {
      return (
        <div
          className="flex min-h-screen flex-col items-center justify-center gap-3 bg-canvas p-6 text-center"
          role="status"
          aria-live="polite"
        >
          <div className="h-8 w-8 animate-spin rounded-full border-2 border-accent border-t-transparent" />
          <p className="text-title font-medium text-primary">Loading local state…</p>
          <p className="text-meta text-muted">正在加载本机状态</p>
        </div>
      );
    }
    if (this.state.error) {
      return (
        <div className="flex min-h-screen items-center justify-center bg-canvas p-6">
          <ErrorState error={this.state.error} onRetry={this.retry} />
        </div>
      );
    }
    return this.props.children;
  }
}
