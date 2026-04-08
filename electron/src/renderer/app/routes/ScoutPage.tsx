import React from 'react';
import { ScoutPage as ScoutPageView } from '#renderer/domains/scout/components/ScoutPage';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';

export function ScoutPage(): React.ReactElement {
  return (
    <div className="absolute inset-0 overflow-auto bg-background px-8 pb-6 pt-[38px]">
      <ErrorBoundary fallbackLabel="Scout view">
        <ScoutPageView active />
      </ErrorBoundary>
    </div>
  );
}
