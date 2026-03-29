import React from 'react';

interface ShortcutEntry {
  keys: string[];
  label: string;
}

const GENERAL: ShortcutEntry[] = [
  { keys: ['⌘', 'K'], label: 'Command palette' },
  { keys: ['⌘', ','], label: 'Settings' },
  { keys: ['⌘', 'N'], label: 'New task' },
  { keys: ['?'], label: 'Shortcut reference' },
  { keys: ['Esc'], label: 'Close / deselect' },
  { keys: ['/'], label: 'Focus search' },
];

const NAVIGATION: ShortcutEntry[] = [
  { keys: ['G', 'C'], label: 'Go to Captain' },
  { keys: ['G', 'D'], label: 'Go to Scout' },
  { keys: ['G', 'S'], label: 'Go to Sessions' },
  { keys: ['J'], label: 'Next item' },
  { keys: ['K'], label: 'Previous item' },
  { keys: ['Enter'], label: 'Expand / open' },
  { keys: ['X'], label: 'Deselect row' },
];

const ACTIONS: ShortcutEntry[] = [
  { keys: ['C'], label: 'Create task' },
  { keys: ['M'], label: 'Merge PR' },
  { keys: ['S'], label: 'Change status' },
  { keys: ['R'], label: 'Restart / rework' },
  { keys: ['T'], label: 'Process scout items' },
  { keys: ['⌘', 'Enter'], label: 'Submit form' },
];

interface Props {
  open: boolean;
  onClose: () => void;
}

export function ShortcutOverlay({ open, onClose }: Props): React.ReactElement | null {
  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[300] flex items-center justify-center"
      data-shortcut-overlay
      style={{ background: 'rgba(0,0,0,0.60)' }}
      onClick={onClose}
    >
      <div
        className="w-[640px] max-w-[90vw] rounded-lg border shadow-xl"
        style={{
          background: 'var(--color-surface-2)',
          borderColor: 'var(--color-border)',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div
          className="flex items-center justify-between border-b px-5 py-3"
          style={{ borderColor: 'var(--color-border)' }}
        >
          <span className="text-[14px] font-semibold" style={{ color: 'var(--color-text-1)' }}>
            Keyboard shortcuts
          </span>
          <button
            onClick={onClose}
            className="text-lg leading-none"
            style={{
              color: 'var(--color-text-3)',
              background: 'none',
              border: 'none',
              cursor: 'pointer',
            }}
          >
            &times;
          </button>
        </div>

        {/* Columns */}
        <div className="grid grid-cols-3 gap-6 px-5 py-4">
          <ShortcutColumn title="General" entries={GENERAL} />
          <ShortcutColumn title="Navigation" entries={NAVIGATION} />
          <ShortcutColumn title="Actions" entries={ACTIONS} />
        </div>

        {/* Footer */}
        <div
          className="border-t px-5 py-2.5 text-[11px]"
          style={{ borderColor: 'var(--color-border)', color: 'var(--color-text-4)' }}
        >
          Press <Kbd>Esc</Kbd> to close
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
  entries: ShortcutEntry[];
}): React.ReactElement {
  return (
    <div>
      <div
        className="mb-2 text-[10px] font-medium uppercase tracking-widest"
        style={{ color: 'var(--color-text-4)' }}
      >
        {title}
      </div>
      <div className="space-y-1.5">
        {entries.map((entry, i) => (
          <div key={i} className="flex items-center justify-between gap-2">
            <span className="text-[12px]" style={{ color: 'var(--color-text-2)' }}>
              {entry.label}
            </span>
            <span className="flex shrink-0 items-center gap-0.5">
              {entry.keys.map((key, ki) => (
                <Kbd key={ki}>{key}</Kbd>
              ))}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function Kbd({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <kbd
      className="inline-flex items-center justify-center rounded px-1.5 py-0.5 text-[10px] font-medium"
      style={{
        background: 'var(--color-surface-3)',
        border: '1px solid var(--color-border)',
        color: 'var(--color-text-2)',
        fontFamily: 'var(--font-mono, Geist Mono, monospace)',
        minWidth: 20,
        textAlign: 'center',
      }}
    >
      {children}
    </kbd>
  );
}
