import React from 'react';
import { useNavigate, useParams } from '@tanstack/react-router';
import { SettingsPage, type SettingsSection } from '#renderer/domains/settings/ui/SettingsPage';
import { ErrorBoundary } from '#renderer/global/ui/ErrorBoundary';
import { isSettingsPath } from '#renderer/global/service/routeHelpers';
import { router } from '#renderer/app/router';

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
              replace: isSettingsPath(router.state.location.pathname),
            })
          }
          onBack={() => {
            if (router.history.canGoBack()) {
              router.history.back();
              return;
            }
            void navigate({ to: '/' });
          }}
        />
      </ErrorBoundary>
    </div>
  );
}
