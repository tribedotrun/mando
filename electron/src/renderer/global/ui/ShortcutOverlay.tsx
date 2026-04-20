import React from 'react';
import { Kbd } from '#renderer/global/ui/kbd';
import { Button } from '#renderer/global/ui/button';

interface ShortcutEntry {
  keys: string[];
  label: string;
}

const GENERAL: readonly ShortcutEntry[] = Object.freeze([
  { keys: ['\u2318', 'K'], label: 'Command palette' },
  { keys: ['\u2318', ','], label: 'Settings' },
  { keys: ['\u2318', 'N'], label: 'New task' },
  { keys: ['?'], label: 'Shortcut reference' },
  { keys: ['Esc'], label: 'Close / deselect' },
]);

const NAVIGATION: readonly ShortcutEntry[] = Object.freeze([
  { keys: ['G', 'C'], label: 'Go to Captain' },
  { keys: ['G', 'D'], label: 'Go to Scout' },
  { keys: ['G', 'S'], label: 'Go to Sessions' },
  { keys: ['\u2318', '['], label: 'Back' },
  { keys: ['\u2318', ']'], label: 'Forward' },
  { keys: ['\u2318', 'B'], label: 'Toggle sidebar' },
  { keys: ['J'], label: 'Next item' },
  { keys: ['K'], label: 'Previous item' },
  { keys: ['Enter'], label: 'Expand / open' },
  { keys: ['X'], label: 'Deselect row' },
]);

const ACTIONS: readonly ShortcutEntry[] = Object.freeze([
  { keys: ['C'], label: 'Create task' },
  { keys: ['M'], label: 'Merge PR' },
  { keys: ['S'], label: 'Change status' },
  { keys: ['R'], label: 'Restart / rework' },
  { keys: ['\u2318', 'Enter'], label: 'Submit form' },
]);

interface Props {
  open: boolean;
  onClose: () => void;
}

export function ShortcutOverlay({ open, onClose }: Props): React.ReactElement | null {
  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[300] flex items-center justify-center bg-overlay"
      data-shortcut-overlay
      onClick={onClose}
    >
      <div
        className="w-[660px] max-w-[90vw] rounded-xl bg-card shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-6 pt-5 pb-1">
          <h2 className="text-sm font-semibold text-foreground">Keyboard shortcuts</h2>
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label="Close keyboard shortcuts"
            onClick={onClose}
            className="text-text-3 hover:text-foreground"
          >
            &times;
          </Button>
        </div>

        <div className="grid grid-cols-3 gap-8 px-6 py-5">
          <ShortcutColumn title="General" entries={GENERAL} />
          <ShortcutColumn title="Navigation" entries={NAVIGATION} />
          <ShortcutColumn title="Actions" entries={ACTIONS} />
        </div>

        <div className="px-6 pb-4 text-xs text-text-4">
          Press <Kbd className="mx-1">Esc</Kbd> to close
        </div>
      </div>
    </div>
  );
}

function ShortcutColumn({
  title,
  entries,
}: {
  title: string;
  entries: readonly ShortcutEntry[];
}): React.ReactElement {
  return (
    <div>
      <div className="text-label mb-3 text-text-3">{title}</div>
      <div className="space-y-2.5">
        {entries.map((entry, i) => (
          <div key={i} className="flex items-center justify-between gap-3">
            <span className="text-caption text-muted-foreground">{entry.label}</span>
            <span className="flex shrink-0 items-center gap-1">
              {entry.keys.map((key, ki) => (
                <Kbd key={ki} className="bg-secondary">
                  {key}
                </Kbd>
              ))}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
