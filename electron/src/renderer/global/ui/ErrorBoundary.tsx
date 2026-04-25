import React from 'react';
import log from '#renderer/global/service/logger';
import { Button } from '#renderer/global/ui/primitives/button';

interface ErrorBoundaryProps {
  children: React.ReactNode;
  fallbackLabel?: string;
}

interface ErrorBoundaryState {
  error: Error | null;
}

export class ErrorBoundary extends React.Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    log.error('[ErrorBoundary] caught:', error.message, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="flex flex-col gap-3 rounded-lg bg-destructive/10 p-6">
          <h3 className="text-sm font-medium text-destructive">
            {this.props.fallbackLabel || 'Component'} failed to render
          </h3>
          <pre className="whitespace-pre-wrap text-xs text-destructive/70">
            {this.state.error.message}
          </pre>
          <Button
            variant="ghost"
            size="xs"
            onClick={() => this.setState({ error: null })}
            className="w-fit bg-destructive/20 text-destructive hover:bg-destructive/30"
          >
            Retry
          </Button>
        </div>
      );
    }
    return this.props.children;
  }
}
