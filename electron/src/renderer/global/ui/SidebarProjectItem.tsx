import React from 'react';
import { DeleteProjectDialog } from '#renderer/global/ui/DeleteProjectDialog';
import type { SidebarChild } from '#renderer/global/service/utils';
import { WorkbenchRow } from '#renderer/global/ui/WorkbenchRow';
import { useSidebarProjectItem } from '#renderer/global/runtime/useSidebarProjectItem';
import {
  ProjectRenameInput,
  ProjectHeaderButton,
} from '#renderer/global/ui/SidebarProjectItemParts';

export type { SidebarChild } from '#renderer/global/service/utils';

interface SidebarProjectItemProps {
  name: string;
  logo?: string | null;
  count: number;
  items?: SidebarChild[];
}

export function SidebarProjectItem({
  name,
  logo,
  count,
  items = [],
}: SidebarProjectItemProps): React.ReactElement {
  const project = useSidebarProjectItem({ name, items });

  return (
    <div
      className="sidebar-project-item relative min-w-0 overflow-hidden"
      data-menu-open={project.menu.open || undefined}
    >
      {project.rename.active ? (
        <ProjectRenameInput
          initialValue={name}
          onCommit={(newName) => void project.rename.commit(newName)}
          onCancel={project.rename.cancel}
        />
      ) : (
        <ProjectHeaderButton
          name={name}
          logo={logo}
          expanded={project.expanded.value}
          menuOpen={project.menu.open}
          actions={project.actions}
          onToggleExpand={() => project.expanded.setValue((v) => !v)}
          onContextMenu={() => project.menu.setOpen(true)}
          onMenuChange={project.menu.setOpen}
          onStartRename={project.rename.start}
          onStartDelete={() => project.delete.setConfirmOpen(true)}
        />
      )}

      <DeleteProjectDialog
        open={project.delete.confirmOpen}
        onOpenChange={project.delete.setConfirmOpen}
        name={name}
        count={count}
      />

      {/* Expanded children: workbench-first rows, sorted by last activity. */}
      {project.expanded.value && items.length > 0 && (
        <div className="flex flex-col gap-0.5 pb-1 pt-0.5">
          {items.map((child) => (
            <WorkbenchRow
              key={`wb:${child.wb.id}:${child.task?.id ?? 'none'}`}
              projectName={name}
              wb={child.wb}
              task={child.task}
              renamingWbId={project.childRename.id}
              setRenamingWbId={project.childRename.setId}
            />
          ))}
        </div>
      )}
    </div>
  );
}
