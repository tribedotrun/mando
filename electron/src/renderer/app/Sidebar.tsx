import React, { useState } from 'react';
import { FileText, List, Settings, Plus, FolderPlus, ChevronUp } from 'lucide-react';
import log from '#renderer/logger';
import { pct } from '#renderer/utils';
import { useTaskStore } from '#renderer/domains/captain';
import { useSettingsStore } from '#renderer/domains/settings';
import { SetupChecklist } from '#renderer/domains/onboarding';
import { SidebarProjectItem } from '#renderer/global/components/SidebarProjectItem';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';

export type Tab = 'captain' | 'scout' | 'sessions';

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
  onAddProject: () => void;
  onRenameProject: (oldName: string, newName: string) => Promise<void>;
  onRemoveProject: (name: string) => Promise<void>;
  onToggleSetup: () => void;
  onDismissSetup: () => void;
  projectFilter: string | null;
  onProjectFilter: (project: string | null) => void;
  setupProgress: SetupProgress | null;
  setupActive: boolean;
}

const NAV_ITEMS: { id: Tab; label: string; Icon: React.FC }[] = [
  { id: 'sessions', label: 'Sessions', Icon: () => <List size={16} /> },
  { id: 'scout', label: 'Scout', Icon: () => <FileText size={16} /> },
];

function UpdateButton(): React.ReactElement | null {
  const [updateReady, setUpdateReady] = useState(false);
  const [installing, setInstalling] = useState(false);

  useMountEffect(() => {
    if (!window.mandoAPI?.updates) return;
    window.mandoAPI.updates.onUpdateReady(() => setUpdateReady(true));
    window.mandoAPI.updates
      .getPending()
      .then((p) => {
        if (p) setUpdateReady(true);
      })
      .catch((err: unknown) => {
        log.warn('[Sidebar] failed to read pending update status:', err);
      });
    return () => window.mandoAPI.updates.removeUpdateListeners();
  });

  if (!updateReady) return null;

  return (
    <button
      disabled={installing}
      onClick={() => {
        setInstalling(true);
        window.mandoAPI.updates
          .installUpdate()
          .catch((err: unknown) => {
            log.error('[Sidebar] install update failed:', err);
            setUpdateReady(false);
          })
          .finally(() => setInstalling(false));
      }}
      className="text-label absolute right-3 top-3 rounded-md px-2 py-1"
      style={
        {
          background: 'var(--color-accent)',
          color: 'var(--color-bg)',
          border: 'none',
          cursor: installing ? 'default' : 'pointer',
          opacity: installing ? 0.6 : 1,
          WebkitAppRegion: 'no-drag',
          zIndex: 20,
        } as React.CSSProperties
      }
    >
      {installing ? 'Installing…' : 'Update'}
    </button>
  );
}

export function Sidebar({
  activeTab,
  onTabChange,
  onNewTask,
  onOpenSettings,
  onAddProject,
  onRenameProject,
  onRemoveProject,
  onToggleSetup,
  onDismissSetup,
  projectFilter,
  onProjectFilter,
  setupProgress,
  setupActive,
}: Props): React.ReactElement {
  const items = useTaskStore((s) => s.items);

  const scoutEnabled = useSettingsStore((s) => !!s.config.features?.scout);
  const visibleNav = scoutEnabled ? NAV_ITEMS : NAV_ITEMS.filter((i) => i.id !== 'scout');

  const configProjects = useSettingsStore((s) => s.config.captain?.projects);

  // Map config project paths → display names, so task.project (a path) resolves
  // to the human-readable name from config.
  const pathToName = React.useMemo(() => {
    const map: Record<string, string> = {};
    if (configProjects) {
      for (const [key, proj] of Object.entries(configProjects)) {
        if (proj.name) {
          map[key] = proj.name;
          if (proj.path && proj.path !== key) map[proj.path] = proj.name;
        }
      }
    }
    return map;
  }, [configProjects]);

  const projectCounts = React.useMemo(() => {
    const counts: Record<string, number> = {};
    for (const item of items) {
      if (item.project) {
        const name = pathToName[item.project] ?? item.project;
        counts[name] = (counts[name] || 0) + 1;
      }
    }
    return counts;
  }, [items, pathToName]);

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

  const projectLogos = React.useMemo(() => {
    const map: Record<string, string> = {};
    if (configProjects) {
      for (const proj of Object.values(configProjects)) {
        if (proj.name && proj.logo) map[proj.name] = proj.logo;
      }
    }
    return map;
  }, [configProjects]);

  const homeActive = activeTab === 'captain' && !projectFilter;

  return (
    <aside
      className="relative flex w-[200px] shrink-0 flex-col"
      style={{
        background: 'var(--color-surface-1)',
        borderRight: '1px solid var(--color-border-subtle)',
        paddingTop: 48,
        paddingBottom: 16,
        paddingLeft: 12,
        paddingRight: 12,
      }}
    >
      <UpdateButton />

      {/* New task — primary action */}
      <button
        onClick={onNewTask}
        className="sidebar-new-task flex w-full items-center gap-2 rounded-button border-none bg-accent px-3 py-2 text-[13px] font-semibold text-bg hover:bg-accent-hover active:bg-accent-pressed"
        style={{ cursor: 'pointer' }}
        data-testid="add-task-btn"
      >
        <Plus size={14} strokeWidth={2} />
        New task
      </button>

      {/* Nav items */}
      <nav
        className="flex flex-col"
        aria-label="Main navigation"
        style={{ paddingTop: 16, gap: 4 }}
      >
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
                padding: '8px 8px',
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
      <div className="flex-1 overflow-auto" style={{ paddingTop: 24 }}>
        <div
          className="text-label flex w-full items-center"
          style={{ padding: '0 0 0 10px', marginBottom: 8 }}
        >
          <button
            data-testid="home-tab"
            onClick={() => {
              onTabChange('captain');
              onProjectFilter(null);
            }}
            className="flex-1 text-left transition-colors"
            style={{
              color: homeActive ? 'var(--color-text-2)' : 'var(--color-text-3)',
              background: 'transparent',
              border: 'none',
              cursor: 'pointer',
              padding: 0,
            }}
          >
            Projects
          </button>
          <button
            data-testid="add-project-sidebar-btn"
            onClick={onAddProject}
            title="Add a new project"
            className="ml-auto flex items-center justify-center text-text-3 transition-colors hover:text-text-2"
            style={{
              width: 20,
              height: 20,
              background: 'transparent',
              border: 'none',
              cursor: 'pointer',
              borderRadius: 4,
              padding: 0,
            }}
          >
            <FolderPlus size={14} />
          </button>
        </div>
        {projects.length > 0 && (
          <div className="flex flex-col" style={{ gap: 4 }}>
            {projects.map((pName) => {
              const isActive = projectFilter === pName;
              return (
                <SidebarProjectItem
                  key={pName}
                  name={pName}
                  logo={projectLogos[pName]}
                  count={projectCounts[pName] ?? 0}
                  active={isActive}
                  onSelect={() => {
                    onTabChange('captain');
                    onProjectFilter(isActive ? null : pName);
                  }}
                  onRename={onRenameProject}
                  onRemove={onRemoveProject}
                />
              );
            })}
          </div>
        )}
      </div>

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
          <Settings size={16} />
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
  const progressPct = pct(progress.completed, progress.total);

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
        aria-label={`${active ? 'Hide' : 'Show'} setup checklist, ${progressPct}% complete`}
        aria-expanded={active}
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
            strokeDasharray={`${(progressPct / 100) * 50.3} 50.3`}
            strokeLinecap="round"
            transform="rotate(-90 10 10)"
          />
        </svg>
        <div className="flex flex-1 flex-col items-start">
          <span className="text-[12px] font-medium text-text-1">
            Get started{' '}
            <span className="text-text-3" style={{ fontWeight: 400 }}>
              {progressPct}%
            </span>
          </span>
          <span className="text-caption truncate text-text-3" style={{ maxWidth: 120 }}>
            {progress.currentStep}
          </span>
        </div>
        <ChevronUp
          size={12}
          color="var(--color-text-3)"
          strokeWidth={2.5}
          style={{
            transform: active ? 'rotate(180deg)' : 'none',
            transition: 'transform 0.15s',
            flexShrink: 0,
          }}
        />
      </button>
    </div>
  );
}
