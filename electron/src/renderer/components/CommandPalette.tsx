import React, { useState, useRef, useMemo, useCallback } from 'react';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { useScrollIntoViewRef } from '#renderer/hooks/useScrollIntoViewRef';

interface Props {
  open: boolean;
  onClose: () => void;
  onAction: (action: string) => void;
}

interface Command {
  id: string;
  name: string;
  shortcut?: string;
  section: 'recent' | 'navigation' | 'actions';
  icon: React.ReactNode;
}

/* ── Row icons (16x16, stroke-based) ── */

function PlusIcon(): React.ReactElement {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <path d="M8 3V13M3 8H13" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}

function MergeIcon(): React.ReactElement {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <circle cx="4" cy="4" r="2" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="12" cy="12" r="2" stroke="currentColor" strokeWidth="1.5" />
      <path d="M4 6V12H10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}

function ArrowRightIcon(): React.ReactElement {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <path
        d="M3 8H13M10 5L13 8L10 11"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function SquareIcon(): React.ReactElement {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <rect x="3" y="3" width="10" height="10" rx="2" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}

function DocIcon(): React.ReactElement {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <rect x="3" y="2" width="10" height="12" rx="2" stroke="currentColor" strokeWidth="1.5" />
      <path d="M6 6H10M6 9H9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}

function CircleIcon(): React.ReactElement {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="5.5" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}

function TargetIcon(): React.ReactElement {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="5.5" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="8" cy="8" r="2" fill="currentColor" />
    </svg>
  );
}

function RefreshIcon(): React.ReactElement {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <path
        d="M3 8a5 5 0 019-3M13 8a5 5 0 01-9 3"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
      <path
        d="M12 2v3h-3M4 14v-3h3"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  );
}

function GearIcon(): React.ReactElement {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="2.5" stroke="currentColor" strokeWidth="1.5" />
      <path
        d="M8 1.5V3M8 13v1.5M1.5 8H3M13 8h1.5M3.1 3.1l1.1 1.1M11.8 11.8l1.1 1.1M3.1 12.9l1.1-1.1M11.8 4.2l1.1-1.1"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  );
}

function ListIcon(): React.ReactElement {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <path
        d="M5 4H13M5 8H13M5 12H13M3 4H3.01M3 8H3.01M3 12H3.01"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  );
}

/* ── Command definitions ── */

const RECENT_COMMANDS: Command[] = [
  {
    id: 'act-create-task',
    name: 'Create new task',
    shortcut: '\u2318N',
    section: 'recent',
    icon: <PlusIcon />,
  },
  { id: 'act-merge', name: 'Merge PR', shortcut: 'M', section: 'recent', icon: <MergeIcon /> },
  {
    id: 'recent-scout',
    name: 'Go to Scout',
    shortcut: 'G D',
    section: 'recent',
    icon: <ArrowRightIcon />,
  },
];

const NAVIGATION_COMMANDS: Command[] = [
  {
    id: 'nav-captain',
    name: 'Captain',
    shortcut: 'G C',
    section: 'navigation',
    icon: <SquareIcon />,
  },
  { id: 'nav-scout', name: 'Scout', shortcut: 'G D', section: 'navigation', icon: <DocIcon /> },
  {
    id: 'nav-sessions',
    name: 'Sessions',
    shortcut: 'G S',
    section: 'navigation',
    icon: <CircleIcon />,
  },
  {
    id: 'nav-cron',
    name: 'Cron Jobs',
    section: 'navigation',
    icon: <CircleIcon />,
  },
  {
    id: 'nav-analytics',
    name: 'Analytics',
    section: 'navigation',
    icon: <CircleIcon />,
  },
];

const ACTION_COMMANDS: Command[] = [
  {
    id: 'act-change-status',
    name: 'Change status\u2026',
    shortcut: 'S',
    section: 'actions',
    icon: <TargetIcon />,
  },
  {
    id: 'act-restart',
    name: 'Restart task',
    shortcut: 'R',
    section: 'actions',
    icon: <RefreshIcon />,
  },
  {
    id: 'act-settings',
    name: 'Open settings',
    shortcut: '\u2318,',
    section: 'actions',
    icon: <GearIcon />,
  },
  {
    id: 'act-process',
    name: 'Process scout items',
    shortcut: 'T',
    section: 'actions',
    icon: <ListIcon />,
  },
];

const ALL_COMMANDS: Command[] = [...RECENT_COMMANDS, ...NAVIGATION_COMMANDS, ...ACTION_COMMANDS];

function fuzzyMatch(query: string, text: string): boolean {
  const lower = text.toLowerCase();
  const q = query.toLowerCase();
  let qi = 0;
  for (let i = 0; i < lower.length && qi < q.length; i++) {
    if (lower[i] === q[qi]) qi++;
  }
  return qi === q.length;
}

function groupBySection(commands: Command[]): Map<string, Command[]> {
  const groups = new Map<string, Command[]>();
  for (const cmd of commands) {
    const label = cmd.section.toUpperCase();
    const list = groups.get(label) ?? [];
    list.push(cmd);
    groups.set(label, list);
  }
  return groups;
}

const SECTION_ORDER = ['RECENT', 'NAVIGATION', 'ACTIONS'];

function ShortcutBadge({ shortcut }: { shortcut: string }): React.ReactElement {
  /* Split multi-key shortcuts like "G C" into separate badges */
  const parts = shortcut.split(/\s+/);
  return (
    <span className="flex items-center gap-1">
      {parts.map((part, i) => (
        <kbd
          key={i}
          className="inline-flex items-center justify-center rounded"
          style={{
            background: 'var(--color-surface-3)',
            color: 'var(--color-text-3)',
            fontSize: 11,
            fontWeight: 500,
            lineHeight: '14px',
            minWidth: 20,
            padding: '2px 6px',
            textAlign: 'center',
          }}
        >
          {part}
        </kbd>
      ))}
    </span>
  );
}

/* ── Inner component — mounted only when open ── */

function CommandPaletteInner({
  onClose,
  onAction,
}: {
  onClose: () => void;
  onAction: (action: string) => void;
}): React.ReactElement {
  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const filtered = useMemo(() => {
    if (!query.trim()) return ALL_COMMANDS;
    return ALL_COMMANDS.filter((cmd) => fuzzyMatch(query, cmd.name));
  }, [query]);

  const grouped = useMemo(() => {
    const groups = groupBySection(filtered);
    const ordered: { label: string; items: Command[] }[] = [];
    for (const section of SECTION_ORDER) {
      const items = groups.get(section);
      if (items?.length) ordered.push({ label: section, items });
    }
    return ordered;
  }, [filtered]);

  const flatItems = useMemo(() => grouped.flatMap((g) => g.items), [grouped]);

  // Focus input on mount
  useMountEffect(() => {
    requestAnimationFrame(() => inputRef.current?.focus());
  });

  // Ref callback: scroll selected item into view
  const scrollRef = useScrollIntoViewRef();

  const executeSelected = useCallback(() => {
    const item = flatItems[selectedIndex];
    if (item) {
      onAction(item.id);
      onClose();
    }
  }, [flatItems, selectedIndex, onAction, onClose]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          setSelectedIndex((i) => (i + 1) % Math.max(flatItems.length, 1));
          break;
        case 'ArrowUp':
          e.preventDefault();
          setSelectedIndex((i) => (i - 1 + flatItems.length) % Math.max(flatItems.length, 1));
          break;
        case 'Enter':
          e.preventDefault();
          executeSelected();
          break;
        case 'Escape':
          e.preventDefault();
          onClose();
          break;
      }
    },
    [flatItems.length, executeSelected, onClose],
  );

  let flatIndex = 0;

  return (
    <div
      className="fixed inset-0 z-[300] flex items-start justify-center pt-[20vh]"
      data-command-palette
      style={{ background: 'rgba(0, 0, 0, 0.60)' }}
      onClick={(e) => e.target === e.currentTarget && onClose()}
      onKeyDown={handleKeyDown}
    >
      <div
        className="flex max-h-[60vh] w-[480px] flex-col overflow-hidden"
        style={{
          background: 'var(--color-surface-2)',
          border: '1px solid var(--color-border)',
          borderRadius: 8,
        }}
      >
        {/* Search input */}
        <div
          className="flex items-center gap-3 px-4"
          style={{ borderBottom: '1px solid var(--color-border)', height: 48 }}
        >
          <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            style={{ flexShrink: 0, color: 'var(--color-text-3)' }}
          >
            <circle cx="7" cy="7" r="5.5" stroke="currentColor" strokeWidth="1.5" />
            <path d="M11 11L14 14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          </svg>
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelectedIndex(0);
            }}
            placeholder="Type a command..."
            className="text-body flex-1 bg-transparent outline-none"
            style={{ color: 'var(--color-text-1)' }}
          />
          <kbd
            className="rounded"
            style={{
              background: 'var(--color-surface-3)',
              color: 'var(--color-text-3)',
              fontSize: 11,
              fontWeight: 500,
              padding: '2px 6px',
            }}
          >
            ESC
          </kbd>
        </div>

        {/* Results */}
        <div className="flex-1 overflow-y-auto py-2">
          {grouped.length === 0 && (
            <div
              className="text-body px-4 py-6 text-center"
              style={{ color: 'var(--color-text-3)' }}
            >
              No commands found
            </div>
          )}
          {grouped.map((group) => (
            <div key={group.label}>
              <div className="text-label px-4 pb-1 pt-3" style={{ color: 'var(--color-text-4)' }}>
                {group.label}
              </div>
              {group.items.map((cmd) => {
                const idx = flatIndex++;
                const isSelected = idx === selectedIndex;
                return (
                  <div
                    key={cmd.id}
                    ref={isSelected ? scrollRef : undefined}
                    data-selected={isSelected}
                    className="mx-2 flex cursor-pointer items-center gap-3 rounded px-3 py-2"
                    style={{
                      background: isSelected ? 'var(--color-surface-3)' : 'transparent',
                      borderRadius: 4,
                    }}
                    onClick={() => {
                      onAction(cmd.id);
                      onClose();
                    }}
                    onMouseEnter={() => setSelectedIndex(idx)}
                  >
                    <span
                      style={{
                        color: isSelected ? 'var(--color-text-2)' : 'var(--color-text-3)',
                        flexShrink: 0,
                      }}
                    >
                      {cmd.icon}
                    </span>
                    <span
                      className="text-body flex-1"
                      style={{
                        color: isSelected ? 'var(--color-text-1)' : 'var(--color-text-2)',
                      }}
                    >
                      {cmd.name}
                    </span>
                    {cmd.shortcut && <ShortcutBadge shortcut={cmd.shortcut} />}
                  </div>
                );
              })}
            </div>
          ))}
        </div>

        {/* Footer */}
        <div
          className="text-caption flex items-center justify-center gap-1 px-4"
          style={{
            height: 36,
            borderTop: '1px solid var(--color-border)',
            color: 'var(--color-text-4)',
          }}
        >
          <span>
            <kbd style={{ color: 'var(--color-text-3)' }}>&uarr;&darr;</kbd> navigate
          </span>
          <span style={{ color: 'var(--color-text-4)', margin: '0 6px' }}>&middot;</span>
          <span>
            <kbd style={{ color: 'var(--color-text-3)' }}>&crarr;</kbd> select
          </span>
          <span style={{ color: 'var(--color-text-4)', margin: '0 6px' }}>&middot;</span>
          <span>
            <kbd style={{ color: 'var(--color-text-3)' }}>esc</kbd> close
          </span>
        </div>
      </div>
    </div>
  );
}

export function CommandPalette({ open, onClose, onAction }: Props): React.ReactElement | null {
  if (!open) return null;
  return <CommandPaletteInner onClose={onClose} onAction={onAction} />;
}
