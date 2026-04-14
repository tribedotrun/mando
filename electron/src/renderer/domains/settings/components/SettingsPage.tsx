import React, { useMemo } from 'react';
import { useConfig } from '#renderer/hooks/queries';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';
import { SettingsGeneral } from '#renderer/domains/settings/components/SettingsGeneral';
import { SettingsProjects } from '#renderer/domains/settings/components/SettingsProjects';
import { SettingsCaptain } from '#renderer/domains/settings/components/SettingsCaptain';
import { SettingsTelegram } from '#renderer/domains/settings/components/SettingsTelegram';
import { SettingsScout } from '#renderer/domains/settings/components/SettingsScout';
import { SettingsExperimental } from '#renderer/domains/settings/components/SettingsExperimental';
import { SettingsAbout } from '#renderer/domains/settings/components/SettingsAbout';
import { SettingsAccounts } from '#renderer/domains/settings/components/SettingsAccounts';
import { Button } from '#renderer/components/ui/button';
import { Skeleton } from '#renderer/components/ui/skeleton';

export type SettingsSection =
  | 'general'
  | 'projects'
  | 'captain'
  | 'credentials'
  | 'telegram'
  | 'scout'
  | 'experimental'
  | 'about';

interface NavItem {
  id: SettingsSection;
  label: string;
}

const BASE_NAV_ITEMS: NavItem[] = [
  { id: 'general', label: 'General' },
  { id: 'projects', label: 'Projects' },
  { id: 'captain', label: 'Captain' },
  { id: 'credentials', label: 'Credentials' },
  { id: 'telegram', label: 'Telegram' },
  { id: 'experimental', label: 'Experimental' },
  { id: 'about', label: 'About' },
];

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
}

export function SettingsPage({
  section: sectionProp = 'general',
  onSectionChange,
}: SettingsPageProps): React.ReactElement {
  const { data: config, isLoading, error } = useConfig();
  const scoutEnabled = !!config?.features?.scout;
  const navItems = useMemo(() => {
    let items = BASE_NAV_ITEMS;
    if (scoutEnabled) {
      const idx = items.findIndex((i) => i.id === 'experimental');
      items = [
        ...items.slice(0, idx),
        { id: 'scout' as SettingsSection, label: 'Scout' },
        ...items.slice(idx),
      ];
    }
    return items;
  }, [scoutEnabled]);

  const section: SettingsSection = navItems.some((item) => item.id === sectionProp)
    ? sectionProp
    : 'general';

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
      <div
        className="absolute inset-x-0 top-0 z-10 h-[38px]"
        style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
      />
      <aside className="flex w-[200px] shrink-0 flex-col bg-card pb-4 pl-3 pr-3 pt-[38px]">
        <div className="mb-4 px-0 text-sm font-medium text-foreground">Settings</div>

        <nav className="flex flex-1 flex-col gap-0.5 overflow-y-auto">
          {navItems.map((item) => {
            const active = section === item.id;
            return (
              <Button
                key={item.id}
                data-testid={`settings-nav-${item.id}`}
                variant="ghost"
                size="sm"
                onClick={() => onSectionChange?.(item.id)}
                className={`w-full justify-start text-[13px] ${
                  active
                    ? 'bg-muted font-medium text-foreground'
                    : 'font-normal text-muted-foreground'
                }`}
              >
                {item.label}
              </Button>
            );
          })}
        </nav>

        {error && (
          <div className="pt-3">
            <p className="text-xs text-destructive">
              {error instanceof Error ? error.message : 'Failed to load config'}
            </p>
          </div>
        )}
      </aside>

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
