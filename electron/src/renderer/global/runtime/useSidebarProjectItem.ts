import { useState } from 'react';
import { useSidebar } from '#renderer/global/runtime/SidebarContext';
import type { SidebarChild } from '#renderer/global/service/utils';

interface Args {
  name: string;
  items: SidebarChild[];
}

export function useSidebarProjectItem({ name, items }: Args) {
  const { state, actions } = useSidebar();
  const [menuOpen, setMenuOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const hasActiveWt = items.some((c) => c.wb.worktree === state.activeTerminalCwd) ?? false;
  const [expanded, setExpanded] = useState(hasActiveWt);
  // Auto-expand when this project gains the active workbench.
  if (hasActiveWt && !expanded) setExpanded(true);
  const [renamingWbId, setRenamingWbId] = useState<number | null>(null);

  const commitRename = async (value: string) => {
    setRenaming(false);
    const trimmed = value.trim();
    if (trimmed && trimmed !== name) {
      await actions.renameProject(name, trimmed);
    }
  };

  const cancelRename = () => {
    setRenaming(false);
  };

  const startRename = () => {
    setRenaming(true);
  };

  return {
    actions,
    menu: { open: menuOpen, setOpen: setMenuOpen },
    rename: {
      active: renaming,
      commit: commitRename,
      cancel: cancelRename,
      start: startRename,
    },
    delete: { confirmOpen, setConfirmOpen },
    expanded: { value: expanded, setValue: setExpanded },
    childRename: { id: renamingWbId, setId: setRenamingWbId },
  };
}
