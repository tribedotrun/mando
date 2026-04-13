import React, { useState } from 'react';
import {
  FileText,
  List,
  Settings,
  SquarePen,
  FolderPlus,
  PanelLeft,
  ArrowLeft,
  ArrowRight,
} from 'lucide-react';
import log from '#renderer/logger';
import { sortProjectChildren, copyToClipboard, getErrorMessage } from '#renderer/utils';
import { useTaskList, useWorkbenchList, useConfig } from '#renderer/hooks/queries';
import { useWorkbenchPin, useWorkbenchRename } from '#renderer/hooks/mutations';
import { toast } from 'sonner';
import {
  SidebarProjectItem,
  type SidebarChild,
} from '#renderer/global/components/SidebarProjectItem';
import { SidebarPinnedSection } from '#renderer/global/components/SidebarPinnedSection';
import { SetupTrigger, type SetupProgress } from '#renderer/app/SetupTrigger';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { Button } from '#renderer/components/ui/button';
import { ScrollArea } from '#renderer/components/ui/scroll-area';
import { Tooltip, TooltipTrigger, TooltipContent } from '#renderer/components/ui/tooltip';
import { Kbd } from '#renderer/components/ui/kbd';
import type { TaskItem } from '#renderer/types';

export type Tab = 'captain' | 'scout' | 'sessions';

export type { SetupProgress };

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
  activeTerminalCwd?: string | null;
  activeTaskId?: number | null;
  onNewTerminal?: (project: string) => void;
  onOpenTask?: (taskId: number) => void;
  onOpenTerminalSession?: (worktree: { project: string; cwd: string }) => void;
  onArchiveWorkbench?: (id: number) => void;
  onToggleSidebar?: () => void;
  onGoBack?: () => void;
  onGoForward?: () => void;
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
  activeTerminalCwd,
  activeTaskId,
  onNewTerminal,
  onOpenTask,
  onOpenTerminalSession,
  onArchiveWorkbench,
  onToggleSidebar,
  onGoBack,
  onGoForward,
}: Props): React.ReactElement | null {
  const { data: taskData } = useTaskList();
  const items = taskData?.items ?? [];
  const { data: workbenches = [] } = useWorkbenchList();
  const pinMut = useWorkbenchPin();
  const renameMut = useWorkbenchRename();

  const openInFinder = React.useCallback((path: string) => {
    window.mandoAPI
      .openInFinder(path)
      .catch((err: unknown) => toast.error(getErrorMessage(err, 'Failed to open in Finder')));
  }, []);
  const copyWorktreePath = React.useCallback((path: string) => {
    void copyToClipboard(path, 'Path copied');
  }, []);
  const renameWorkbench = React.useCallback(
    (id: number, title: string) => {
      if (renameMut.isPending) return;
      renameMut.mutate({ id, title });
    },
    [renameMut],
  );

  const { data: _config } = useConfig();
  const scoutEnabled = !!_config?.features?.scout;
  const visibleNav = scoutEnabled ? NAV_ITEMS : NAV_ITEMS.filter((i) => i.id !== 'scout');

  const configProjects = _config?.captain?.projects;

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

  // Build a map from workbench ID to its task (if any).
  const wbTaskMap = React.useMemo(() => {
    const map = new Map<number, TaskItem>();
    for (const task of items) {
      if (task.workbench_id) map.set(task.workbench_id, task);
    }
    return map;
  }, [items]);

  // Pinned workbenches: sorted by pinnedAt DESC (most recent first).
  const pinnedItems = React.useMemo(() => {
    return workbenches
      .filter((wb) => wb.pinnedAt && !wb.archivedAt)
      .sort((a, b) => (b.pinnedAt! > a.pinnedAt! ? 1 : b.pinnedAt! < a.pinnedAt! ? -1 : 0))
      .map((wb) => ({
        wb,
        task: wbTaskMap.get(wb.id),
        project: pathToName[wb.project] ?? wb.project,
      }));
  }, [workbenches, wbTaskMap, pathToName]);

  const pinnedWbIds = React.useMemo(() => new Set(pinnedItems.map((p) => p.wb.id)), [pinnedItems]);

  const { projectCounts, projectChildren } = React.useMemo(() => {
    const counts: Record<string, number> = {};
    const children: Record<string, SidebarChild[]> = {};
    const taskWbIds = new Set(items.filter((t) => t.workbench_id).map((t) => t.workbench_id));

    for (const item of items) {
      if (!item.project) continue;
      // Exclude tasks whose workbench is pinned (shown in pinned section).
      if (item.workbench_id && pinnedWbIds.has(item.workbench_id)) continue;
      const pName = pathToName[item.project] ?? item.project;
      counts[pName] = (counts[pName] || 0) + 1;
      (children[pName] ??= []).push({ kind: 'task', task: item });
    }

    // Taskless, non-archived, non-pinned workbenches interleave with tasks.
    for (const wb of workbenches) {
      if (taskWbIds.has(wb.id) || wb.archivedAt || pinnedWbIds.has(wb.id)) continue;
      const pName = pathToName[wb.project] ?? wb.project;
      (children[pName] ??= []).push({ kind: 'workbench', wb });
    }

    for (const [key, arr] of Object.entries(children)) {
      children[key] = sortProjectChildren(arr);
    }
    return { projectCounts: counts, projectChildren: children };
  }, [items, workbenches, pathToName, pinnedWbIds]);

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

  return (
    <aside className="relative flex h-full flex-col overflow-hidden bg-card px-1.5">
      {/* Window controls toolbar (next to traffic lights) */}
      <div
        className="flex h-[38px] shrink-0 items-start pl-[70px] pt-[10px]"
        style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
      >
        <div
          className="flex items-center gap-1"
          style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
        >
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={onToggleSidebar}
                className="flex h-6 w-6 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
              >
                <PanelLeft size={14} />
              </button>
            </TooltipTrigger>
            <TooltipContent
              side="bottom"
              className="flex items-center gap-3 px-3 py-2 text-sm font-medium"
            >
              Toggle sidebar <Kbd>&#8984;B</Kbd>
            </TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={onGoBack}
                className="flex h-6 w-6 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
              >
                <ArrowLeft size={14} />
              </button>
            </TooltipTrigger>
            <TooltipContent
              side="bottom"
              className="flex items-center gap-3 px-3 py-2 text-sm font-medium"
            >
              Back <Kbd>&#8984;[</Kbd>
            </TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={onGoForward}
                className="flex h-6 w-6 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
              >
                <ArrowRight size={14} />
              </button>
            </TooltipTrigger>
            <TooltipContent
              side="bottom"
              className="flex items-center gap-3 px-3 py-2 text-sm font-medium"
            >
              Forward <Kbd>&#8984;]</Kbd>
            </TooltipContent>
          </Tooltip>
        </div>
      </div>

      <UpdateButton />

      <ScrollArea className="min-h-0 flex-1">
        {/* New task */}
        <button
          onClick={onNewTask}
          className="sidebar-new-task flex w-full items-center gap-2 rounded-md px-1.5 py-2 text-[13px] text-muted-foreground transition-colors hover:text-foreground"
          data-testid="add-task-btn"
        >
          <SquarePen size={16} strokeWidth={1.5} />
          New task
        </button>

        {/* Nav items */}
        <nav className="flex flex-col gap-1 pt-1" aria-label="Main navigation">
          {visibleNav.map(({ id, label, Icon }) => {
            const active = activeTab === id && !projectFilter;
            return (
              <Button
                key={id}
                variant="ghost"
                size="sm"
                data-testid={`${id}-tab`}
                onClick={() => onTabChange(id)}
                className={`w-full justify-start gap-2 px-1.5 has-[>svg]:px-1.5 text-[13px] ${
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

        {/* Pinned workbenches */}
        {pinnedItems.length > 0 && (
          <div className="pt-6">
            <SidebarPinnedSection
              items={pinnedItems}
              activeTerminalCwd={activeTerminalCwd}
              activeTaskId={activeTaskId}
              onOpenTask={onOpenTask}
              onOpenTerminalSession={onOpenTerminalSession}
              onUnpin={(id) => !pinMut.isPending && pinMut.mutate({ id, pinned: false })}
              onPin={(id) => !pinMut.isPending && pinMut.mutate({ id, pinned: true })}
              onArchiveWorkbench={onArchiveWorkbench}
              onRenameWorkbench={renameWorkbench}
              onOpenWorkbenchInFinder={openInFinder}
              onCopyWorkbenchPath={copyWorktreePath}
            />
          </div>
        )}

        {/* Projects section */}
        <div className={pinnedItems.length > 0 ? 'pt-3' : 'pt-6'}>
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
                return (
                  <SidebarProjectItem
                    key={pName}
                    name={pName}
                    logo={projectLogos[pName]}
                    count={projectCounts[pName] ?? 0}
                    onRename={onRenameProject}
                    onRemove={onRemoveProject}
                    onNewTerminal={onNewTerminal}
                    items={projectChildren[pName] ?? []}
                    activeWorktreeCwd={activeTerminalCwd}
                    onOpenWorktree={onOpenTerminalSession}
                    onOpenTask={onOpenTask}
                    onArchiveWorkbench={onArchiveWorkbench}
                    onPinWorkbench={(id) =>
                      !pinMut.isPending && pinMut.mutate({ id, pinned: true })
                    }
                    onUnpinWorkbench={(id) =>
                      !pinMut.isPending && pinMut.mutate({ id, pinned: false })
                    }
                    onRenameWorkbench={renameWorkbench}
                    onOpenWorkbenchInFinder={openInFinder}
                    onCopyWorkbenchPath={copyWorktreePath}
                  />
                );
              })}
            </div>
          )}
        </div>
      </ScrollArea>

      {/* Settings */}
      <Button
        variant="ghost"
        size="sm"
        data-testid="settings-gear"
        onClick={onOpenSettings}
        className="w-full justify-start gap-2 px-1.5 has-[>svg]:px-1.5 text-[13px] font-normal text-text-3"
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
