import React from 'react';
import { useTaskStore } from '#renderer/stores/taskStore';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import { SetupChecklist } from '#renderer/components/SetupChecklist';

export type Tab = 'captain' | 'scout' | 'sessions' | 'cron' | 'analytics';

export interface SetupProgress {
  completed: number;
  total: number;
  currentStep: string;
}

interface Props {
  activeTab: Tab;
  onTabChange: (tab: Tab) => void;
  onNewTask: () => void;
  onOpenSettings: () => void;
  onToggleSetup: () => void;
  onDismissSetup: () => void;
  projectFilter: string | null;
  onProjectFilter: (project: string | null) => void;
  setupProgress: SetupProgress | null;
  setupActive: boolean;
}

function ScoutIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor">
      <rect x="2" y="2" width="12" height="12" rx="2" strokeWidth="1.5" />
      <path d="M5 6h6M5 8.5h4" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}

function SessionsIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor">
      <path d="M3 4h10M3 8h10M3 12h6" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}

function SettingsIcon() {
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 01-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z" />
    </svg>
  );
}

function CronIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor">
      <circle cx="8" cy="8" r="6" strokeWidth="1.5" />
      <path d="M8 5v3l2 2" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function AnalyticsIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor">
      <path d="M3 13V8" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M7 13V5" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M11 13V3" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}

const NAV_ITEMS: { id: Tab; label: string; Icon: React.FC; featureFlag?: string }[] = [
  { id: 'scout', label: 'Scout', Icon: ScoutIcon },
  { id: 'sessions', label: 'Sessions', Icon: SessionsIcon },
  { id: 'cron', label: 'Cron', Icon: CronIcon },
  { id: 'analytics', label: 'Analytics', Icon: AnalyticsIcon, featureFlag: 'analytics' },
];

export function Sidebar({
  activeTab,
  onTabChange,
  onNewTask,
  onOpenSettings,
  onToggleSetup,
  onDismissSetup,
  projectFilter,
  onProjectFilter,
  setupProgress,
  setupActive,
}: Props): React.ReactElement {
  const items = useTaskStore((s) => s.items);
  const features = useSettingsStore((s) => s.config.features);

  const visibleNav = React.useMemo(
    () =>
      NAV_ITEMS.filter((item) => {
        if (!item.featureFlag) return true;
        return !!(features as Record<string, unknown> | undefined)?.[item.featureFlag];
      }),
    [features],
  );

  const configProjects = useSettingsStore((s) => s.config.captain?.projects);

  const projectCounts = React.useMemo(() => {
    const counts: Record<string, number> = {};
    for (const item of items) {
      if (item.project) {
        counts[item.project] = (counts[item.project] || 0) + 1;
      }
    }
    return counts;
  }, [items]);

  const projects = React.useMemo(() => {
    const names = new Set(Object.keys(projectCounts));
    // Include configured projects even if they have no tasks yet
    if (configProjects) {
      for (const proj of Object.values(configProjects)) {
        if (proj.name) names.add(proj.name);
      }
    }
    return [...names].sort();
  }, [projectCounts, configProjects]);

  const homeActive = activeTab === 'captain' && !projectFilter;

  return (
    <aside
      className="flex w-[200px] shrink-0 flex-col"
      style={{
        background: 'var(--color-surface-1)',
        borderRight: '1px solid var(--color-border-subtle)',
        paddingTop: 48,
        paddingBottom: 16,
        paddingLeft: 12,
        paddingRight: 12,
      }}
    >
      {/* New task — primary action */}
      <button
        onClick={onNewTask}
        className="flex w-full items-center text-[13px] transition-colors"
        style={{
          background: 'var(--color-surface-2)',
          color: 'var(--color-text-2)',
          borderRadius: 'var(--radius-button)',
          padding: '7px 10px',
          border: 'none',
          cursor: 'pointer',
        }}
        data-testid="add-task-btn"
      >
        New task
      </button>

      {/* Nav items */}
      <nav className="flex flex-col" style={{ paddingTop: 16, gap: 4 }}>
        {visibleNav.map(({ id, label, Icon }) => {
          const active = activeTab === id && !projectFilter;
          return (
            <button
              key={id}
              data-testid={`${id}-tab`}
              onClick={() => {
                onTabChange(id);
                onProjectFilter(null);
              }}
              className="flex items-center gap-2 text-[13px] transition-colors"
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
              <Icon />
              {label}
            </button>
          );
        })}
      </nav>

      {/* Projects section */}
      {projects.length > 0 && (
        <div className="flex-1 overflow-auto" style={{ paddingTop: 24 }}>
          <button
            data-testid="home-tab"
            onClick={() => {
              onTabChange('captain');
              onProjectFilter(null);
            }}
            className="text-label flex w-full items-center justify-between transition-colors"
            style={{
              color: homeActive ? 'var(--color-text-2)' : 'var(--color-text-3)',
              padding: '0 10px',
              marginBottom: 8,
              background: 'transparent',
              border: 'none',
              cursor: 'pointer',
            }}
          >
            <span>Projects</span>
          </button>
          <div className="flex flex-col" style={{ gap: 4 }}>
            {projects.map((name) => {
              const active = projectFilter === name;
              return (
                <button
                  key={name}
                  onClick={() => {
                    onTabChange('captain');
                    onProjectFilter(active ? null : name);
                  }}
                  className="flex items-center justify-between text-[13px] transition-colors"
                  style={{
                    background: active ? 'var(--color-surface-2)' : 'transparent',
                    color: active ? 'var(--color-text-1)' : '#C4BDD8',
                    fontWeight: active ? 500 : 400,
                    padding: '6px 10px',
                    borderRadius: 5,
                    border: 'none',
                    cursor: 'pointer',
                  }}
                >
                  <span className="truncate">{name}</span>
                  <span
                    className="shrink-0"
                    style={{
                      fontSize: 11,
                      color: 'var(--color-text-3)',
                      marginLeft: 4,
                    }}
                  >
                    {projectCounts[name]}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* Spacer */}
      {projects.length === 0 && <div className="flex-1" />}

      {/* Settings */}
      <div style={{ borderTop: '1px solid var(--color-border-subtle)', paddingTop: 12 }}>
        <button
          data-testid="settings-gear"
          onClick={onOpenSettings}
          className="flex w-full items-center gap-2 text-[13px] transition-colors"
          style={{
            color: 'var(--color-text-3)',
            padding: '4px 10px',
            background: 'transparent',
            border: 'none',
            cursor: 'pointer',
          }}
        >
          <SettingsIcon />
          Settings
        </button>
      </div>

      {/* Setup trigger + popover */}
      {setupProgress && (
        <SetupTrigger
          progress={setupProgress}
          active={setupActive}
          onToggle={onToggleSetup}
          onDismiss={onDismissSetup}
        />
      )}
    </aside>
  );
}

// ---------------------------------------------------------------------------
// Setup trigger bar + floating popover
// ---------------------------------------------------------------------------

function SetupTrigger({
  progress,
  active,
  onToggle,
  onDismiss,
}: {
  progress: SetupProgress;
  active: boolean;
  onToggle: () => void;
  onDismiss: () => void;
}): React.ReactElement {
  const pct = Math.round((progress.completed / progress.total) * 100);

  return (
    <div style={{ position: 'relative', marginTop: 8 }}>
      {/* Popover card */}
      {active && (
        <div
          data-testid="setup-popover"
          style={{
            position: 'absolute',
            bottom: 'calc(100% + 6px)',
            left: 0,
            width: 300,
            maxHeight: 420,
            overflowY: 'auto',
            borderRadius: 8,
            background: 'var(--color-surface-2)',
            border: '1px solid var(--color-border-subtle)',
            boxShadow: '0 -4px 20px rgba(0,0,0,0.5)',
            zIndex: 200,
          }}
        >
          <SetupChecklist onDismiss={onDismiss} onMinimize={onToggle} />
        </div>
      )}

      {/* Trigger bar */}
      <button
        data-testid="setup-trigger"
        onClick={onToggle}
        className="flex w-full items-center transition-colors"
        style={{
          padding: '8px 10px',
          borderRadius: 6,
          background: active ? 'var(--color-surface-2)' : 'transparent',
          border: 'none',
          cursor: 'pointer',
          gap: 8,
        }}
      >
        {/* Mini progress ring */}
        <svg width="16" height="16" viewBox="0 0 20 20" style={{ flexShrink: 0 }}>
          <circle
            cx="10"
            cy="10"
            r="8"
            fill="none"
            stroke="var(--color-surface-3)"
            strokeWidth="2"
          />
          <circle
            cx="10"
            cy="10"
            r="8"
            fill="none"
            stroke="var(--color-accent)"
            strokeWidth="2"
            strokeDasharray={`${(pct / 100) * 50.3} 50.3`}
            strokeLinecap="round"
            transform="rotate(-90 10 10)"
          />
        </svg>
        <div className="flex flex-1 flex-col items-start">
          <span className="text-[12px] font-medium" style={{ color: 'var(--color-text-1)' }}>
            Get started{' '}
            <span style={{ color: 'var(--color-text-3)', fontWeight: 400 }}>{pct}%</span>
          </span>
          <span
            className="truncate text-[11px]"
            style={{ color: 'var(--color-text-3)', maxWidth: 120 }}
          >
            {progress.currentStep}
          </span>
        </div>
        <svg
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="var(--color-text-3)"
          style={{
            transform: active ? 'rotate(180deg)' : 'none',
            transition: 'transform 0.15s',
            flexShrink: 0,
          }}
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 15l7-7 7 7" />
        </svg>
      </button>
    </div>
  );
}
