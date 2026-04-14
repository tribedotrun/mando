import React from 'react';
import { useNavigate, useParams } from '@tanstack/react-router';
import {
  SettingsPage,
  type SettingsSection,
} from '#renderer/domains/settings/components/SettingsPage';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';

export function SettingsPageRoute(): React.ReactElement {
  const navigate = useNavigate();
  const { section } = useParams({ strict: false }) as { section: string };

  return (
    <div className="flex-1 overflow-hidden">
      <ErrorBoundary fallbackLabel="Settings">
        <SettingsPage
          section={(section as SettingsSection) ?? 'general'}
          onSectionChange={(s) =>
            void navigate({
              to: '/settings/$section',
              params: { section: s },
            })
          }
        />
      </ErrorBoundary>
    </div>
  );
}
