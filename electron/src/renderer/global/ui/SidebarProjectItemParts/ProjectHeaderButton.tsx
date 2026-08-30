import React from 'react';
import { MoreHorizontal, Pencil, Trash2, ChevronRight } from 'lucide-react';
import { projectLogoUrl } from '#renderer/global/runtime/useApi';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from '#renderer/global/ui/primitives/dropdown-menu';
import { Button } from '#renderer/global/ui/primitives/button';

function ProjectLogo({ logo }: { logo: string }): React.ReactElement {
  return (
    <img
      key={logo}
      src={projectLogoUrl(logo)}
      alt=""
      width={16}
      height={16}
      className="shrink-0 rounded-sm object-contain"
      onError={(e) => {
        (e.target as HTMLImageElement).style.display = 'none';
      }}
    />
  );
}

interface ProjectHeaderButtonProps {
  name: string;
  logo?: string | null;
  expanded: boolean;
  menuOpen: boolean;
  onToggleExpand: () => void;
  onContextMenu: () => void;
  onMenuChange: (open: boolean) => void;
  onStartRename: () => void;
  onStartDelete: () => void;
}

export function ProjectHeaderButton({
  name,
  logo,
  expanded,
  menuOpen,
  onToggleExpand,
  onContextMenu,
  onMenuChange,
  onStartRename,
  onStartDelete,
}: ProjectHeaderButtonProps): React.ReactElement {
  return (
    <DropdownMenu open={menuOpen} onOpenChange={onMenuChange}>
      <Button
        variant="ghost"
        onClick={onToggleExpand}
        onContextMenu={(e) => {
          e.preventDefault();
          onContextMenu();
        }}
        data-testid="project-header"
        data-project-name={name}
        data-expanded={expanded || undefined}
        className="flex h-auto w-full items-center justify-between rounded-md px-1.5 py-1.5 text-[13px] font-normal text-muted-foreground transition-colors"
      >
        <span className="flex min-w-0 items-center gap-1.5">
          <ChevronRight
            size={10}
            className={`shrink-0 transition-transform duration-150 ${expanded ? 'rotate-90' : ''}`}
          />
          {logo && <ProjectLogo logo={logo} />}
          <span className="truncate">{name}</span>
        </span>
        <span className="sidebar-project-dots flex shrink-0 items-center gap-1">
          <DropdownMenuTrigger asChild>
            <span
              role="button"
              tabIndex={-1}
              onClick={(e) => e.stopPropagation()}
              className="flex size-5 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
            >
              <MoreHorizontal size={14} />
            </span>
          </DropdownMenuTrigger>
        </span>
      </Button>
      <DropdownMenuContent align="end" className="min-w-[130px]">
        <DropdownMenuItem onSelect={onStartRename}>
          <Pencil size={12} />
          Rename
        </DropdownMenuItem>
        <DropdownMenuItem variant="destructive" onSelect={onStartDelete}>
          <Trash2 size={12} />
          Remove
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
