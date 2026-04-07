import React, { useState } from 'react';
import { FileText, List, Settings, Plus, FolderPlus, ChevronUp, PanelLeft } from 'lucide-react';
import log from '#renderer/logger';
import { pct } from '#renderer/utils';
import { useTaskStore } from '#renderer/domains/captain';
import { useSettingsStore } from '#renderer/domains/settings';
import { SetupChecklist } from '#renderer/domains/onboarding';
import { SidebarProjectItem } from '#renderer/global/components/SidebarProjectItem';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { Button } from '#renderer/components/ui/button';
import { ScrollArea } from '#renderer/components/ui/scroll-area';
import { Separator } from '#renderer/components/ui/separator';

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
  collapsed: boolean;
  onToggleCollapse: () => void;
  onNewTerminal?: (project: string) => void;
  onOpenTask?: (taskId: number) => void;
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
    <Button
      size="xs"
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
      className="absolute right-3 top-3 z-20"
      style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
    >
      {installing ? 'Installing\u2026' : 'Update'}
    </Button>
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
  collapsed,
  onToggleCollapse,
  onNewTerminal,
  onOpenTask,
}: Props): React.ReactElement | null {
  const items = useTaskStore((s) => s.items);

  const scoutEnabled = useSettingsStore((s) => !!s.config.features?.scout);
  const visibleNav = scoutEnabled ? NAV_ITEMS : NAV_ITEMS.filter((i) => i.id !== 'scout');

  const configProjects = useSettingsStore((s) => s.config.captain?.projects);

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

  const { projectCounts, projectTasks } = React.useMemo(() => {
    const counts: Record<string, number> = {};
    const tasks: Record<string, typeof items> = {};
    for (const item of items) {
      if (item.project) {
        const pName = pathToName[item.project] ?? item.project;
        counts[pName] = (counts[pName] || 0) + 1;
        (tasks[pName] ??= []).push(item);
      }
    }
    return { projectCounts: counts, projectTasks: tasks };
  }, [items, pathToName]);

  const projects = React.useMemo(() => {
    const names = new Set(Object.keys(projectCounts));
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

  if (collapsed) {
    return (
      <div className="flex h-full w-full items-start justify-center bg-card pt-3">
        <Button
          variant="ghost"
          size="icon-xs"
          onClick={onToggleCollapse}
          title="Expand sidebar (Cmd+B)"
          className="text-text-3 hover:text-text-2"
          style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
        >
          <PanelLeft size={14} />
        </Button>
      </div>
    );
  }

  return (
    <aside className="relative flex h-full flex-col bg-card px-2 pb-4 pt-12">
      <UpdateButton />

      {/* Collapse toggle */}
      <Button
        variant="ghost"
        size="icon-xs"
        onClick={onToggleCollapse}
        title="Toggle sidebar (Cmd+B)"
        className="absolute left-[70px] top-3 z-20 text-text-3 hover:text-text-2"
        style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
      >
        <PanelLeft size={14} />
      </Button>

      {/* New task */}
      <Button
        onClick={onNewTask}
        className="sidebar-new-task w-full gap-2"
        size="sm"
        data-testid="add-task-btn"
      >
        <Plus size={14} strokeWidth={2} />
        New task
      </Button>

      {/* Nav items */}
      <nav className="flex flex-col gap-1 pt-4" aria-label="Main navigation">
        {visibleNav.map(({ id, label, Icon }) => {
          const active = activeTab === id && !projectFilter;
          return (
            <Button
              key={id}
              variant="ghost"
              size="sm"
              data-testid={`${id}-tab`}
              onClick={() => {
                onTabChange(id);
                onProjectFilter(null);
              }}
              className={`w-full justify-start gap-2 text-[13px] ${
                active
                  ? 'bg-muted font-medium text-foreground'
                  : 'font-normal text-muted-foreground'
              }`}
            >
              <Icon />
              {label}
            </Button>
          );
        })}
      </nav>

      {/* Projects section */}
      <ScrollArea className="flex-1 pt-6">
        <div className="text-label mb-2 flex w-full items-center pl-1.5">
          <Button
            variant="ghost"
            size="xs"
            data-testid="home-tab"
            onClick={() => {
              onTabChange('captain');
              onProjectFilter(null);
            }}
            className={`h-auto flex-1 justify-start p-0 text-left transition-colors ${homeActive ? 'text-muted-foreground' : 'text-text-3'}`}
          >
            Projects
          </Button>
          <Button
            variant="ghost"
            size="icon-xs"
            data-testid="add-project-sidebar-btn"
            onClick={onAddProject}
            className="ml-auto text-text-3 hover:text-muted-foreground"
          >
            <FolderPlus size={14} />
          </Button>
        </div>
        {projects.length > 0 && (
          <div className="flex flex-col gap-1">
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
                  onNewTerminal={onNewTerminal}
                  tasks={projectTasks[pName] ?? []}
                  onOpenTask={onOpenTask}
                />
              );
            })}
          </div>
        )}
      </ScrollArea>

      {/* Settings */}
      <Separator className="my-3" />
      <Button
        variant="ghost"
        size="sm"
        data-testid="settings-gear"
        onClick={onOpenSettings}
        className="w-full justify-start gap-2 text-[13px] font-normal text-text-3"
      >
        <Settings size={16} />
        Settings
      </Button>

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
    <div className="relative mt-2">
      {/* Popover card */}
      {active && (
        <div
          data-testid="setup-popover"
          className="absolute bottom-[calc(100%+6px)] left-0 z-[200] w-[300px] max-h-[420px] overflow-y-auto rounded-lg bg-muted shadow-[0_-4px_20px_rgba(0,0,0,0.5)]"
        >
          <SetupChecklist onDismiss={onDismiss} onMinimize={onToggle} />
        </div>
      )}

      {/* Trigger bar */}
      <Button
        variant="ghost"
        data-testid="setup-trigger"
        onClick={onToggle}
        aria-label={`${active ? 'Hide' : 'Show'} setup checklist, ${progressPct}% complete`}
        aria-expanded={active}
        className={`flex h-auto w-full items-center gap-2 rounded-md px-2.5 py-2 transition-colors ${active ? 'bg-muted' : 'bg-transparent'}`}
      >
        {/* Mini progress ring */}
        <svg width="16" height="16" viewBox="0 0 20 20" className="shrink-0">
          <circle cx="10" cy="10" r="8" fill="none" stroke="var(--secondary)" strokeWidth="2" />
          <circle
            cx="10"
            cy="10"
            r="8"
            fill="none"
            stroke="var(--primary)"
            strokeWidth="2"
            strokeDasharray={`${(progressPct / 100) * 50.3} 50.3`}
            strokeLinecap="round"
            transform="rotate(-90 10 10)"
          />
        </svg>
        <div className="flex flex-1 flex-col items-start">
          <span className="text-[12px] font-medium text-foreground">
            Get started <span className="font-normal text-text-3">{progressPct}%</span>
          </span>
          <span className="text-caption max-w-[120px] truncate text-text-3">
            {progress.currentStep}
          </span>
        </div>
        <ChevronUp
          size={12}
          color="var(--text-3)"
          strokeWidth={2.5}
          className={`shrink-0 transition-transform duration-150 ${active ? 'rotate-180' : ''}`}
        />
      </Button>
    </div>
  );
}
