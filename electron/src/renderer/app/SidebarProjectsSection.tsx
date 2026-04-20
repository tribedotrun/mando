import React, { useState } from 'react';
import { Check, Filter, FolderPlus } from 'lucide-react';
import { SidebarProjectItem } from '#renderer/global/ui/SidebarProjectItem';
import { Button } from '#renderer/global/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '#renderer/global/ui/dropdown-menu';
import { WORKBENCH_FILTER_OPTIONS, type WorkbenchStatusFilter } from '#renderer/global/types';
import type { SidebarChild } from '#renderer/global/service/utils';

interface Props {
  homeActive: boolean;
  onGoHome: () => void;
  onAddProject: () => void;
  projects: string[];
  projectLogos: Record<string, string>;
  projectCounts: Record<string, number>;
  projectChildren: Record<string, SidebarChild[]>;
  workbenchFilter: WorkbenchStatusFilter;
  onFilterChange: (filter: WorkbenchStatusFilter) => void;
}

export function SidebarProjectsSection({
  homeActive,
  onGoHome,
  onAddProject,
  projects,
  projectLogos,
  projectCounts,
  projectChildren,
  workbenchFilter,
  onFilterChange,
}: Props): React.ReactElement {
  const [filterMenuOpen, setFilterMenuOpen] = useState(false);

  return (
    <div>
      <div className="text-label mb-2 flex w-full items-center pl-1.5">
        <Button
          variant="ghost"
          size="xs"
          data-testid="home-tab"
          onClick={onGoHome}
          className={`h-auto flex-1 justify-start p-0 text-left transition-colors ${homeActive ? 'text-muted-foreground' : 'text-text-3'}`}
        >
          Projects
        </Button>
        <DropdownMenu open={filterMenuOpen} onOpenChange={setFilterMenuOpen}>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              data-testid="workbench-filter-btn"
              aria-label="Filter projects by status"
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
                  onSelect={() => onFilterChange(opt)}
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
          aria-label="Add project"
          onClick={onAddProject}
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
  );
}
