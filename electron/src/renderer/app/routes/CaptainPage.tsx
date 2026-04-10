import React from 'react';
import { CaptainView } from '#renderer/domains/captain/components/CaptainView';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';

export function CaptainPage(): React.ReactElement {
  return (
    <div className="absolute inset-0 overflow-auto bg-background px-4 pb-2">
      <ErrorBoundary fallbackLabel="Captain view">
        <CaptainView active />
      </ErrorBoundary>
    </div>
  );
}
