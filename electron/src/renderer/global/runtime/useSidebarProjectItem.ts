import { useState, useCallback, useRef } from 'react';
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
  const [renameValue, setRenameValue] = useState(name);
  const submittedRef = useRef(false);
  const [renamingWbId, setRenamingWbId] = useState<number | null>(null);

  const inputRefCb = useCallback((el: HTMLInputElement | null) => {
    if (el) {
      el.focus();
      el.select();
    }
  }, []);

  const submitRename = async () => {
    if (submittedRef.current) return;
    submittedRef.current = true;
    setRenaming(false);
    const trimmed = renameValue.trim();
    if (trimmed && trimmed !== name) {
      await actions.renameProject(name, trimmed);
    }
  };

  const cancelRename = () => {
    submittedRef.current = true;
    setRenaming(false);
    setRenameValue(name);
  };

  const startRename = () => {
    submittedRef.current = false;
    setRenaming(true);
    setRenameValue(name);
  };

  return {
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
  };
}
