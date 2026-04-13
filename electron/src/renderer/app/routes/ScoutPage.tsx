import React from 'react';
import { useNavigate, useSearch } from '@tanstack/react-router';
import { ScoutPage as ScoutPageView } from '#renderer/domains/scout/components/ScoutPage';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';

export function ScoutPage(): React.ReactElement {
  const navigate = useNavigate();
  const { item } = useSearch({ strict: false }) as { item?: number };
  const activeItemId = typeof item === 'number' ? item : null;

  return (
    <div className="absolute inset-0 overflow-auto bg-background px-8 pb-6">
      <ErrorBoundary fallbackLabel="Scout view">
        <ScoutPageView
          active
          activeItemId={activeItemId}
          onOpenItem={(id) =>
            void navigate({
              to: '/scout',
              search: { item: id },
            })
          }
          onBackToList={() => void navigate({ to: '/scout', search: {} })}
        />
      </ErrorBoundary>
    </div>
  );
}
