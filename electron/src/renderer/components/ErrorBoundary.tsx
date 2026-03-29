import React from 'react';
import log from '#renderer/logger';

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
        <div
          className="rounded-lg border p-6"
          style={{
            borderColor: 'color-mix(in srgb, var(--color-error) 50%, transparent)',
            backgroundColor: 'var(--color-error-bg)',
          }}
        >
          <h3 className="text-sm font-medium" style={{ color: 'var(--color-error)' }}>
            {this.props.fallbackLabel || 'Component'} failed to render
          </h3>
          <pre
            className="mt-2 whitespace-pre-wrap text-xs"
            style={{ color: 'var(--color-error)', opacity: 0.7 }}
          >
            {this.state.error.message}
          </pre>
          <button
            onClick={() => this.setState({ error: null })}
            className="mt-3 rounded-md px-3 py-1.5 text-xs"
            style={{
              backgroundColor: 'color-mix(in srgb, var(--color-error) 40%, transparent)',
              color: 'var(--color-error)',
            }}
          >
            Retry
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
