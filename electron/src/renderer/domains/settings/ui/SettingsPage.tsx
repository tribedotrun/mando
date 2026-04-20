import React from 'react';
import { ErrorBoundary } from '#renderer/global/ui/ErrorBoundary';
import { SettingsGeneral } from '#renderer/domains/settings/ui/SettingsGeneral';
import { SettingsProjects } from '#renderer/domains/settings/ui/SettingsProjects';
import { SettingsCaptain } from '#renderer/domains/settings/ui/SettingsCaptain';
import { SettingsTelegram } from '#renderer/domains/settings/ui/SettingsTelegram';
import { SettingsScout } from '#renderer/domains/settings/ui/SettingsScout';
import { SettingsExperimental } from '#renderer/domains/settings/ui/SettingsExperimental';
import { SettingsAbout } from '#renderer/domains/settings/ui/SettingsAbout';
import { SettingsAccounts } from '#renderer/domains/settings/ui/SettingsAccounts';
import { Skeleton } from '#renderer/global/ui/skeleton';
import { SettingsSidebar } from '#renderer/domains/settings/ui/SettingsPageParts';
import { useSettingsPage } from '#renderer/domains/settings/runtime/useSettingsPage';
export type { SettingsSection } from '#renderer/domains/settings/types';
import type { SettingsSection } from '#renderer/domains/settings/types';

function SettingsPanel({ section }: { section: SettingsSection }) {
  switch (section) {
    case 'general':
      return <SettingsGeneral />;
    case 'projects':
      return <SettingsProjects />;
    case 'captain':
      return <SettingsCaptain />;
    case 'credentials':
      return <SettingsAccounts />;
    case 'telegram':
      return <SettingsTelegram />;
    case 'scout':
      return <SettingsScout />;
    case 'experimental':
      return <SettingsExperimental />;
    case 'about':
      return <SettingsAbout />;
  }
}

interface SettingsPageProps {
  section?: SettingsSection;
  onSectionChange?: (section: SettingsSection) => void;
  onBack: () => void;
}

export function SettingsPage({
  section: sectionProp = 'general',
  onSectionChange,
  onBack,
}: SettingsPageProps): React.ReactElement {
  const { navItems, section, isLoading, error } = useSettingsPage(sectionProp);

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="flex flex-col items-center gap-3">
          <Skeleton className="h-5 w-32" />
          <Skeleton className="h-4 w-20" />
        </div>
      </div>
    );
  }

  return (
    <div data-testid="settings-page" className="relative flex h-full">
      <div className="absolute inset-x-0 top-0 z-10 h-[38px]" style={{ WebkitAppRegion: 'drag' }} />
      <SettingsSidebar
        navItems={navItems}
        section={section}
        error={error}
        onBack={onBack}
        onSectionChange={onSectionChange}
      />
      <main className="flex-1 overflow-y-auto px-8 pb-6 pt-[38px]">
        <div className="max-w-[720px]">
          <ErrorBoundary key={section} fallbackLabel={section}>
            <SettingsPanel section={section} />
          </ErrorBoundary>
        </div>
      </main>
    </div>
  );
}
