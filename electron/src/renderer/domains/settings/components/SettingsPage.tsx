import React, { useMemo, useState } from 'react';
import { ChevronLeft } from 'lucide-react';
import { useSettingsStore } from '#renderer/domains/settings/stores/settingsStore';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';
import { SettingsGeneral } from '#renderer/domains/settings/components/SettingsGeneral';
import { SettingsProjects } from '#renderer/domains/settings/components/SettingsProjects';
import { SettingsCaptain } from '#renderer/domains/settings/components/SettingsCaptain';
import { SettingsTelegram } from '#renderer/domains/settings/components/SettingsTelegram';
import { SettingsScout } from '#renderer/domains/settings/components/SettingsScout';
import { SettingsExperimental } from '#renderer/domains/settings/components/SettingsExperimental';
import { SettingsAbout } from '#renderer/domains/settings/components/SettingsAbout';
import { Button } from '#renderer/components/ui/button';
import { Skeleton } from '#renderer/components/ui/skeleton';

export type SettingsSection =
  | 'general'
  | 'projects'
  | 'captain'
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
  onBack: () => void;
  initialSection?: SettingsSection;
}

export function SettingsPage({
  onBack,
  initialSection = 'general',
}: SettingsPageProps): React.ReactElement {
  const [section, setSection] = useState<SettingsSection>(initialSection);
  const load = useSettingsStore((s) => s.load);
  const loading = useSettingsStore((s) => s.loading);
  const error = useSettingsStore((s) => s.error);
  const saveSuccess = useSettingsStore((s) => s.saveSuccess);
  const scoutEnabled = useSettingsStore((s) => !!s.config.features?.scout);
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

  useMountEffect(() => {
    void load();
  });

  if (loading) {
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
    <div data-testid="settings-page" className="flex h-full">
      <aside className="flex w-[200px] shrink-0 flex-col bg-card pb-4 pl-3 pr-3 pt-[38px]">
        <Button
          data-testid="settings-back"
          variant="ghost"
          size="sm"
          onClick={onBack}
          className="mb-4 justify-start gap-2 px-0 text-sm font-medium text-foreground"
        >
          <ChevronLeft size={14} />
          Settings
        </Button>

        <nav className="flex flex-1 flex-col gap-0.5 overflow-y-auto">
          {navItems.map((item) => {
            const active = section === item.id;
            return (
              <Button
                key={item.id}
                data-testid={`settings-nav-${item.id}`}
                variant="ghost"
                size="sm"
                onClick={() => setSection(item.id)}
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

        {(error || saveSuccess) && (
          <div className="pt-3">
            {error && <p className="text-xs text-destructive">{error}</p>}
            {saveSuccess && <p className="text-xs text-success">Saved</p>}
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
