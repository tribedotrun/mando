import React from 'react';
import { ArrowLeft } from 'lucide-react';
import { Button } from '#renderer/global/ui/button';
import type { SettingsSection } from '#renderer/domains/settings/types';
import type { NavItem } from '#renderer/domains/settings/runtime/useSettingsPage';

interface SettingsSidebarProps {
  navItems: readonly NavItem[];
  section: SettingsSection;
  error: Error | null;
  onBack: () => void;
  onSectionChange?: (section: SettingsSection) => void;
}

export function SettingsSidebar({
  navItems,
  section,
  error,
  onBack,
  onSectionChange,
}: SettingsSidebarProps): React.ReactElement {
  return (
    <aside className="flex w-[200px] shrink-0 flex-col bg-card pb-4 pl-3 pr-3 pt-[38px]">
      <button
        type="button"
        onClick={onBack}
        className="mb-2 flex items-center gap-1.5 px-0 text-xs text-muted-foreground transition-colors hover:text-foreground"
      >
        <ArrowLeft size={14} />
        Back to app
      </button>
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
  );
}
