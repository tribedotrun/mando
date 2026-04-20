import React, { useCallback } from 'react';
import { useRouterState } from '@tanstack/react-router';
import { useTaskActions } from '#renderer/domains/captain';
import { useUIStore } from '#renderer/global/runtime/useUIStore';
import { CommandPalette } from '#renderer/global/ui/CommandPalette';
import { CreateTaskModal } from '#renderer/domains/captain/ui/AddTaskForm';
import { MergeModal } from '#renderer/domains/captain/ui/MergeModal';
import { ShortcutOverlay } from '#renderer/global/ui/ShortcutOverlay';

interface Props {
  onPaletteAction: (action: string) => void;
}

export function RootShellOverlays({ onPaletteAction }: Props): React.ReactElement {
  const actions = useTaskActions();
  const paletteOpen = useUIStore((s) => s.paletteOpen);
  const createTaskOpen = useUIStore((s) => s.createTaskOpen);
  const shortcutsOpen = useUIStore((s) => s.shortcutsOpen);
  const mergeItem = useUIStore((s) => s.mergeItem);

  const currentProject = useRouterState({
    select: (s) => (s.location.search as { project?: string }).project ?? null,
  });

  const handleMergeConfirm = useCallback(
    (itemId: number, pr: number, project: string) => {
      useUIStore.getState().setMergeItem(null);
      void actions.handleMerge(itemId, pr, project);
    },
    [actions],
  );

  return (
    <>
      {mergeItem && (
        <MergeModal
          item={mergeItem}
          onConfirm={handleMergeConfirm}
          onCancel={() => useUIStore.getState().setMergeItem(null)}
        />
      )}
      <CommandPalette
        open={paletteOpen}
        onClose={() => useUIStore.getState().closePalette()}
        onAction={onPaletteAction}
      />
      <CreateTaskModal
        open={createTaskOpen}
        onClose={() => useUIStore.getState().closeCreateTask()}
        initialProject={currentProject}
      />
      <ShortcutOverlay
        open={shortcutsOpen}
        onClose={() => useUIStore.getState().closeShortcuts()}
      />
    </>
  );
}
