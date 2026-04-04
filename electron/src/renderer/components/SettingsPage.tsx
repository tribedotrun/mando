import React, { useState } from 'react';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { ErrorBoundary } from '#renderer/components/ErrorBoundary';
import { SettingsGeneral } from '#renderer/components/SettingsGeneral';
import { SettingsProjects } from '#renderer/components/SettingsProjects';
import { SettingsCaptain } from '#renderer/components/SettingsCaptain';
import { SettingsTelegram } from '#renderer/components/SettingsTelegram';
import { SettingsScout } from '#renderer/components/SettingsScout';
import { SettingsExperimental } from '#renderer/components/SettingsExperimental';
import { SettingsAbout } from '#renderer/components/SettingsAbout';
import { SetupChecklist } from '#renderer/components/SetupChecklist';

export type SettingsSection =
  | 'setup'
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
  { id: 'setup', label: 'Setup' },
  { id: 'general', label: 'General' },
  { id: 'projects', label: 'Projects' },
  { id: 'captain', label: 'Captain' },
  { id: 'telegram', label: 'Telegram' },
  { id: 'experimental', label: 'Experimental' },
  { id: 'about', label: 'About' },
];

function SettingsPanel({ section }: { section: SettingsSection }) {
  switch (section) {
    case 'setup':
      return (
        <SetupChecklist
          onDismiss={() => {
            const store = useSettingsStore.getState();
            store.updateSection('features', { setupDismissed: true });
            store.save();
          }}
        />
      );
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
  const setupDismissed = useSettingsStore((s) => !!s.config.features?.setupDismissed);
  const navItems = (() => {
    let items = setupDismissed ? BASE_NAV_ITEMS.filter((i) => i.id !== 'setup') : BASE_NAV_ITEMS;
    if (scoutEnabled) {
      const idx = items.findIndex((i) => i.id === 'experimental');
      items = [
        ...items.slice(0, idx),
        { id: 'scout' as SettingsSection, label: 'Scout' },
        ...items.slice(idx),
      ];
    }
    return items;
  })();

  useMountEffect(() => {
    load();
  });

  if (loading) {
    return (
      <div
        className="flex h-full items-center justify-center"
        style={{ color: 'var(--color-text-3)' }}
      >
        Loading settings...
      </div>
    );
  }

  return (
    <div data-testid="settings-page" className="flex h-full">
      <aside
        className="flex w-[200px] shrink-0 flex-col"
        style={{
          borderRight: '1px solid var(--color-border-subtle)',
          background: 'var(--color-surface-1)',
          paddingTop: 38,
          paddingBottom: 16,
          paddingLeft: 12,
          paddingRight: 12,
        }}
      >
        <button
          data-testid="settings-back"
          onClick={onBack}
          className="flex items-center transition-colors"
          style={{
            gap: 8,
            color: 'var(--color-text-1)',
            background: 'transparent',
            border: 'none',
            cursor: 'pointer',
            padding: '0 0 16px 0',
            fontSize: 14,
            fontWeight: 500,
          }}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M15 19l-7-7 7-7"
            />
          </svg>
          Settings
        </button>

        <nav className="flex flex-1 flex-col overflow-y-auto" style={{ gap: 1 }}>
          {navItems.map((item) => {
            const active = section === item.id;
            return (
              <button
                key={item.id}
                data-testid={`settings-nav-${item.id}`}
                onClick={() => setSection(item.id)}
                className="flex w-full items-center text-[13px] transition-colors"
                style={{
                  background: active ? 'var(--color-surface-2)' : 'transparent',
                  color: active ? 'var(--color-text-1)' : 'var(--color-text-2)',
                  fontWeight: active ? 500 : 400,
                  padding: '7px 10px',
                  borderRadius: 6,
                  border: 'none',
                  cursor: 'pointer',
                }}
              >
                {item.label}
              </button>
            );
          })}
        </nav>

        {(error || saveSuccess) && (
          <div style={{ paddingTop: 12, borderTop: '1px solid var(--color-border-subtle)' }}>
            {error && (
              <p className="text-xs" style={{ color: 'var(--color-error)' }}>
                {error}
              </p>
            )}
            {saveSuccess && (
              <p className="text-xs" style={{ color: 'var(--color-success)' }}>
                Saved
              </p>
            )}
          </div>
        )}
      </aside>

      <main className="flex-1 overflow-y-auto" style={{ padding: '38px 32px 24px' }}>
        <div style={{ maxWidth: 720 }}>
          <ErrorBoundary key={section} fallbackLabel={section}>
            <SettingsPanel section={section} />
          </ErrorBoundary>
        </div>
      </main>
    </div>
  );
}
