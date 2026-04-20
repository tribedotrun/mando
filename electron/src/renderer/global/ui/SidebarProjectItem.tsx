import React from 'react';
import { DeleteProjectDialog } from '#renderer/global/ui/DeleteProjectDialog';
import type { SidebarChild } from '#renderer/global/service/utils';
import { WorkbenchRow } from '#renderer/global/ui/SidebarChildRows';
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
  const {
    actions,
    menuOpen,
    setMenuOpen,
    renaming,
    confirmOpen,
    setConfirmOpen,
    expanded,
    setExpanded,
    renameValue,
    setRenameValue,
    renamingWbId,
    setRenamingWbId,
    inputRefCb,
    submitRename,
    cancelRename,
    startRename,
  } = useSidebarProjectItem({ name, items });

  return (
    <div
      className="sidebar-project-item relative min-w-0 overflow-hidden"
      data-menu-open={menuOpen || undefined}
    >
      {renaming ? (
        <ProjectRenameInput
          value={renameValue}
          inputRefCb={inputRefCb}
          onChange={setRenameValue}
          onSubmit={() => void submitRename()}
          onCancel={cancelRename}
        />
      ) : (
        <ProjectHeaderButton
          name={name}
          logo={logo}
          expanded={expanded}
          menuOpen={menuOpen}
          actions={actions}
          onToggleExpand={() => setExpanded((v) => !v)}
          onContextMenu={() => setMenuOpen(true)}
          onMenuChange={setMenuOpen}
          onStartRename={startRename}
          onStartDelete={() => setConfirmOpen(true)}
        />
      )}

      <DeleteProjectDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        name={name}
        count={count}
      />

      {/* Expanded children: workbench-first rows, sorted by last activity. */}
      {expanded && items.length > 0 && (
        <div className="flex flex-col gap-0.5 pb-1 pt-0.5">
          {items.map((child) => (
            <WorkbenchRow
              key={`wb:${child.wb.id}:${child.task?.id ?? 'none'}`}
              projectName={name}
              wb={child.wb}
              task={child.task}
              renamingWbId={renamingWbId}
              setRenamingWbId={setRenamingWbId}
            />
          ))}
        </div>
      )}
    </div>
  );
}
