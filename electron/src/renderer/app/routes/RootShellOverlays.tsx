import React, { useCallback } from 'react';
import { MergeModal, useTaskActions } from '#renderer/domains/captain/shell';
import { useUIStore } from '#renderer/global/runtime/useUIStore';
import { CommandPalette } from '#renderer/global/ui/CommandPalette';
import { ShortcutOverlay } from '#renderer/global/ui/ShortcutOverlay';

interface Props {
  onPaletteAction: (action: string) => void;
}

export function RootShellOverlays({ onPaletteAction }: Props): React.ReactElement {
  const actions = useTaskActions();
  const paletteOpen = useUIStore((s) => s.paletteOpen);
  const shortcutsOpen = useUIStore((s) => s.shortcutsOpen);
  const mergeItem = useUIStore((s) => s.mergeItem);

  const handleMergeConfirm = useCallback(
    (itemId: number, pr: number, project: string) => {
      useUIStore.getState().setMergeItem(null);
      void actions.merge.handleMerge(itemId, pr, project);
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
      <ShortcutOverlay
        open={shortcutsOpen}
        onClose={() => useUIStore.getState().closeShortcuts()}
      />
    </>
  );
}
