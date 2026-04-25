import React, { useState } from 'react';
import {
  FileText,
  List,
  Settings,
  SquarePen,
  PanelLeft,
  ArrowLeft,
  ArrowRight,
} from 'lucide-react';
import { SidebarNavButton, SidebarUpdateButton } from '#renderer/app/SidebarControls';
import { useSidebarData } from '#renderer/domains/captain/shell';
import { SetupProgressButton } from '#renderer/domains/onboarding/shell';
import { SidebarPinnedSection } from '#renderer/global/ui/SidebarPinnedSection';
import { SidebarProjectsSection } from '#renderer/app/SidebarProjectsSection';
import { Button } from '#renderer/global/ui/primitives/button';
import { ScrollArea } from '#renderer/global/ui/primitives/scroll-area';
import { type WorkbenchStatusFilter } from '#renderer/global/types';
import { useSidebar, type Tab } from '#renderer/global/runtime/SidebarContext';

export type { Tab, SetupProgress } from '#renderer/global/runtime/SidebarContext';

const NAV_ITEMS: readonly { id: Tab; label: string; Icon: React.FC }[] = Object.freeze([
  { id: 'sessions', label: 'Sessions', Icon: () => <List size={16} /> },
  { id: 'scout', label: 'Scout', Icon: () => <FileText size={16} /> },
]);

export function Sidebar(): React.ReactElement | null {
  const { state, actions } = useSidebar();
  const [workbenchFilter, setWorkbenchFilter] = useState<WorkbenchStatusFilter>('active');

  const { pinnedItems, projectCounts, projectChildren, projects, projectLogos, scoutEnabled } =
    useSidebarData(workbenchFilter);
  const visibleNav = scoutEnabled ? NAV_ITEMS : NAV_ITEMS.filter((i) => i.id !== 'scout');
  const homeActive = state.activeTab === 'captain' && !state.projectFilter;

  return (
    <aside className="relative flex h-full flex-col overflow-hidden bg-card px-1.5">
      <div
        className="flex h-[38px] shrink-0 items-start pl-[70px] pt-[10px]"
        style={{ WebkitAppRegion: 'drag' }}
      >
        <div className="flex items-center gap-1" style={{ WebkitAppRegion: 'no-drag' }}>
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
          <SidebarProjectsSection
            homeActive={homeActive}
            onGoHome={() => {
              actions.changeTab('captain');
              actions.filterByProject(null);
            }}
            onAddProject={actions.addProject}
            projects={projects}
            projectLogos={projectLogos}
            projectCounts={projectCounts}
            projectChildren={projectChildren}
            workbenchFilter={workbenchFilter}
            onFilterChange={setWorkbenchFilter}
          />
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
        <SetupProgressButton
          progress={state.setupProgress}
          active={state.setupActive}
          onToggle={actions.toggleSetup}
          onDismiss={actions.dismissSetup}
        />
      )}
    </aside>
  );
}
