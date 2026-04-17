import React, { useState } from 'react';
import {
  Check,
  FileText,
  Filter,
  List,
  Settings,
  SquarePen,
  FolderPlus,
  PanelLeft,
  ArrowLeft,
  ArrowRight,
} from 'lucide-react';
import { SidebarNavButton, SidebarUpdateButton } from '#renderer/app/SidebarControls';
import { useSidebarData } from '#renderer/app/useSidebarData';
import { SidebarProjectItem } from '#renderer/global/ui/SidebarProjectItem';
import { SidebarPinnedSection } from '#renderer/global/ui/SidebarPinnedSection';
import { SetupTrigger } from '#renderer/app/SetupTrigger';
import { Button } from '#renderer/global/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '#renderer/global/ui/dropdown-menu';
import { ScrollArea } from '#renderer/global/ui/scroll-area';
import { WORKBENCH_FILTER_OPTIONS, type WorkbenchStatusFilter } from '#renderer/global/types';
import { useSidebar, type Tab } from '#renderer/global/runtime/SidebarContext';

export type { Tab, SetupProgress } from '#renderer/global/runtime/SidebarContext';

const NAV_ITEMS: { id: Tab; label: string; Icon: React.FC }[] = [
  { id: 'sessions', label: 'Sessions', Icon: () => <List size={16} /> },
  { id: 'scout', label: 'Scout', Icon: () => <FileText size={16} /> },
];

export function Sidebar(): React.ReactElement | null {
  const { state, actions } = useSidebar();
  const [workbenchFilter, setWorkbenchFilter] = useState<WorkbenchStatusFilter>('active');
  const [filterMenuOpen, setFilterMenuOpen] = useState(false);

  const { pinnedItems, projectCounts, projectChildren, projects, projectLogos, scoutEnabled } =
    useSidebarData(workbenchFilter);
  const visibleNav = scoutEnabled ? NAV_ITEMS : NAV_ITEMS.filter((i) => i.id !== 'scout');
  const homeActive = state.activeTab === 'captain' && !state.projectFilter;

  return (
    <aside className="relative flex h-full flex-col overflow-hidden bg-card px-1.5">
      <div
        className="flex h-[38px] shrink-0 items-start pl-[70px] pt-[10px]"
        style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
      >
        <div
          className="flex items-center gap-1"
          style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
        >
          <SidebarNavButton
            onClick={actions.toggleSidebar}
            icon={PanelLeft}
            label="Toggle sidebar"
            shortcut="&#8984;B"
          />
          <SidebarNavButton
            onClick={actions.goBack}
            icon={ArrowLeft}
            label="Back"
            shortcut="&#8984;["
          />
          <SidebarNavButton
            onClick={actions.goForward}
            icon={ArrowRight}
            label="Forward"
            shortcut="&#8984;]"
          />
        </div>
      </div>

      <SidebarUpdateButton />
      <ScrollArea className="min-h-0 flex-1">
        <button
          onClick={actions.newTask}
          className="sidebar-new-task flex w-full items-center gap-2 rounded-md px-1.5 py-2 text-[13px] text-muted-foreground transition-colors hover:text-foreground"
          data-testid="add-task-btn"
        >
          <SquarePen size={16} strokeWidth={1.5} />
          New task
        </button>
        <nav className="flex flex-col gap-1 pt-1" aria-label="Main navigation">
          {visibleNav.map(({ id, label, Icon }) => {
            const active = state.activeTab === id && !state.projectFilter;
            return (
              <Button
                key={id}
                variant="ghost"
                size="sm"
                data-testid={`${id}-tab`}
                onClick={() => actions.changeTab(id)}
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
        {pinnedItems.length > 0 && (
          <div className="pt-6">
            <SidebarPinnedSection items={pinnedItems} />
          </div>
        )}
        <div className={pinnedItems.length > 0 ? 'pt-3' : 'pt-6'}>
          <div className="text-label mb-2 flex w-full items-center pl-1.5">
            <Button
              variant="ghost"
              size="xs"
              data-testid="home-tab"
              onClick={() => {
                actions.changeTab('captain');
                actions.filterByProject(null);
              }}
              className={`h-auto flex-1 justify-start p-0 text-left transition-colors ${homeActive ? 'text-muted-foreground' : 'text-text-3'}`}
            >
              Projects
            </Button>
            <DropdownMenu open={filterMenuOpen} onOpenChange={setFilterMenuOpen}>
              <DropdownMenuTrigger asChild>
                <button
                  data-testid="workbench-filter-btn"
                  className={`ml-auto flex h-5 w-5 items-center justify-center rounded transition-colors hover:text-muted-foreground ${
                    workbenchFilter !== 'active' ? 'text-foreground' : 'text-text-3'
                  }`}
                >
                  <Filter size={12} />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuLabel>Status</DropdownMenuLabel>
                {WORKBENCH_FILTER_OPTIONS.map((opt) => {
                  const active = workbenchFilter === opt;
                  return (
                    <DropdownMenuItem
                      key={opt}
                      onSelect={() => setWorkbenchFilter(opt)}
                      className={active ? 'text-foreground' : ''}
                    >
                      <span className="flex-1 capitalize">{opt}</span>
                      {active && <Check size={14} />}
                    </DropdownMenuItem>
                  );
                })}
              </DropdownMenuContent>
            </DropdownMenu>
            <Button
              variant="ghost"
              size="icon-xs"
              data-testid="add-project-sidebar-btn"
              onClick={actions.addProject}
              className="text-text-3 hover:text-muted-foreground"
            >
              <FolderPlus size={14} />
            </Button>
          </div>
          {projects.length > 0 && (
            <div className="flex flex-col gap-1">
              {projects.map((pName) => (
                <SidebarProjectItem
                  key={pName}
                  name={pName}
                  logo={projectLogos[pName]}
                  count={projectCounts[pName] ?? 0}
                  items={projectChildren[pName] ?? []}
                />
              ))}
            </div>
          )}
        </div>
      </ScrollArea>

      <Button
        variant="ghost"
        size="sm"
        data-testid="settings-gear"
        onClick={actions.openSettings}
        className="w-full justify-start gap-2 px-1.5 has-[>svg]:px-1.5 text-[13px] font-normal text-text-3"
      >
        <Settings size={16} />
        Settings
      </Button>

      {state.setupProgress && (
        <SetupTrigger
          progress={state.setupProgress}
          active={state.setupActive}
          onToggle={actions.toggleSetup}
          onDismiss={actions.dismissSetup}
        />
      )}
    </aside>
  );
}
