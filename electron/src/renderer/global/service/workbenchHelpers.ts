/** Shared rename-commit handler for workbench title editing. */
export function commitWorkbenchRename(
  newTitle: string,
  currentTitle: string,
  wbId: number,
  renameWorkbench: (id: number, title: string) => void,
  clearRenaming: () => void,
) {
  clearRenaming();
  const trimmed = newTitle.trim();
  if (trimmed && trimmed !== currentTitle) renameWorkbench(wbId, trimmed);
}
