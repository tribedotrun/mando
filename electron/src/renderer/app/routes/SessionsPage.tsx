import React from 'react';
import { SessionsCard } from '#renderer/domains/sessions/components/SessionsCard';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';

export function SessionsPage(): React.ReactElement {
  return (
    <div className="absolute inset-0 overflow-auto bg-background px-8 pb-6 pt-[38px]">
      <ErrorBoundary fallbackLabel="Sessions view">
        <SessionsCard active />
      </ErrorBoundary>
    </div>
  );
}
