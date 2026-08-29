import { Component, type ErrorInfo, type ReactNode } from 'react';
import { ErrorState } from '@/components/shared/ErrorState';

type Props = {
  children: ReactNode;
};

type State = {
  error: Error | null;
};

/** Catches render-tree failures so one page crash does not blank the whole app. */
export class AppErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('AppErrorBoundary', error, info.componentStack);
  }

  private retry = () => {
    this.setState({ error: null });
  };

  render() {
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
