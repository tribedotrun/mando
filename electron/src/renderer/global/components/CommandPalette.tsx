import React, { useRef } from 'react';
import { Command } from 'cmdk';
import {
  Plus,
  GitMerge,
  ArrowRight,
  Square,
  FileText,
  Circle,
  Target,
  RefreshCw,
  Settings,
  Search,
} from 'lucide-react';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';

interface Props {
  open: boolean;
  onClose: () => void;
  onAction: (action: string) => void;
}

interface CommandDef {
  id: string;
  name: string;
  shortcut?: string;
  section: 'recent' | 'navigation' | 'actions';
  icon: React.ReactNode;
}

/* -- Command definitions -- */

const RECENT_COMMANDS: CommandDef[] = [
  {
    id: 'act-create-task',
    name: 'Create new task',
    shortcut: '\u2318N',
    section: 'recent',
    icon: <Plus size={16} />,
  },
  {
    id: 'act-merge',
    name: 'Merge PR',
    shortcut: 'M',
    section: 'recent',
    icon: <GitMerge size={16} />,
  },
  {
    id: 'recent-scout',
    name: 'Go to Scout',
    shortcut: 'G D',
    section: 'recent',
    icon: <ArrowRight size={16} />,
  },
];

const NAVIGATION_COMMANDS: CommandDef[] = [
  {
    id: 'nav-captain',
    name: 'Captain',
    shortcut: 'G C',
    section: 'navigation',
    icon: <Square size={16} />,
  },
  {
    id: 'nav-scout',
    name: 'Scout',
    shortcut: 'G D',
    section: 'navigation',
    icon: <FileText size={16} />,
  },
  {
    id: 'nav-sessions',
    name: 'Sessions',
    shortcut: 'G S',
    section: 'navigation',
    icon: <Circle size={16} />,
  },
];

const ACTION_COMMANDS: CommandDef[] = [
  {
    id: 'act-change-status',
    name: 'Change status\u2026',
    shortcut: 'S',
    section: 'actions',
    icon: <Target size={16} />,
  },
  {
    id: 'act-restart',
    name: 'Restart task',
    shortcut: 'R',
    section: 'actions',
    icon: <RefreshCw size={16} />,
  },
  {
    id: 'act-settings',
    name: 'Open settings',
    shortcut: '\u2318,',
    section: 'actions',
    icon: <Settings size={16} />,
  },
];

function ShortcutBadge({ shortcut }: { shortcut: string }): React.ReactElement {
  const parts = shortcut.split(/\s+/);
  return (
    <span className="flex items-center gap-1">
      {parts.map((part, i) => (
        <kbd
          key={i}
          className="inline-flex items-center justify-center rounded bg-surface-3 text-text-3"
          style={{
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

/* -- Command row -- */

function CommandRow({
  cmd,
  onSelect,
}: {
  cmd: CommandDef;
  onSelect: () => void;
}): React.ReactElement {
  return (
    <Command.Item
      value={cmd.name}
      onSelect={onSelect}
      className="mx-2 flex cursor-pointer items-center gap-3 rounded px-3 py-2 data-[selected=true]:bg-surface-3"
      style={{ borderRadius: 4 }}
    >
      <span className="shrink-0 text-text-3">{cmd.icon}</span>
      <span className="text-body flex-1 text-text-2">{cmd.name}</span>
      {cmd.shortcut && <ShortcutBadge shortcut={cmd.shortcut} />}
    </Command.Item>
  );
}

/* -- Inner component -- mounted only when open -- */

function CommandPaletteInner({
  onClose,
  onAction,
}: {
  onClose: () => void;
  onAction: (action: string) => void;
}): React.ReactElement {
  const inputRef = useRef<HTMLInputElement>(null);

  useMountEffect(() => {
    requestAnimationFrame(() => inputRef.current?.focus());
  });

  function handleSelect(id: string): void {
    onAction(id);
    onClose();
  }

  return (
    <div
      className="fixed inset-0 z-[300] flex items-start justify-center pt-[20vh] bg-overlay"
      data-command-palette
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <Command
        shouldFilter={true}
        className="flex max-h-[60vh] w-[480px] flex-col overflow-hidden"
        style={{
          background: 'var(--color-surface-2)',
          border: '1px solid var(--color-border)',
          borderRadius: 8,
        }}
        onKeyDown={(e: React.KeyboardEvent) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            onClose();
          }
        }}
      >
        {/* Search input */}
        <div
          className="flex items-center gap-3 px-4"
          style={{ borderBottom: '1px solid var(--color-border)', height: 48 }}
        >
          <Search size={16} className="text-text-3" style={{ flexShrink: 0 }} />
          <Command.Input
            ref={inputRef}
            placeholder="Type a command..."
            className="text-body flex-1 bg-transparent text-text-1 outline-none"
          />
          <kbd
            className="rounded bg-surface-3 text-text-3"
            style={{
              fontSize: 11,
              fontWeight: 500,
              padding: '2px 6px',
            }}
          >
            ESC
          </kbd>
        </div>

        {/* Results */}
        <Command.List className="flex-1 overflow-y-auto py-2">
          <Command.Empty>
            <div className="text-body px-4 py-6 text-center text-text-3">No commands found</div>
          </Command.Empty>

          <Command.Group
            heading="RECENT"
            className="[&_[cmdk-group-heading]]:text-label [&_[cmdk-group-heading]]:px-4 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:pt-3 [&_[cmdk-group-heading]]:text-text-4"
          >
            {RECENT_COMMANDS.map((cmd) => (
              <CommandRow key={cmd.id} cmd={cmd} onSelect={() => handleSelect(cmd.id)} />
            ))}
          </Command.Group>

          <Command.Group
            heading="NAVIGATION"
            className="[&_[cmdk-group-heading]]:text-label [&_[cmdk-group-heading]]:px-4 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:pt-3 [&_[cmdk-group-heading]]:text-text-4"
          >
            {NAVIGATION_COMMANDS.map((cmd) => (
              <CommandRow key={cmd.id} cmd={cmd} onSelect={() => handleSelect(cmd.id)} />
            ))}
          </Command.Group>

          <Command.Group
            heading="ACTIONS"
            className="[&_[cmdk-group-heading]]:text-label [&_[cmdk-group-heading]]:px-4 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:pt-3 [&_[cmdk-group-heading]]:text-text-4"
          >
            {ACTION_COMMANDS.map((cmd) => (
              <CommandRow key={cmd.id} cmd={cmd} onSelect={() => handleSelect(cmd.id)} />
            ))}
          </Command.Group>
        </Command.List>

        {/* Footer */}
        <div
          className="text-caption flex items-center justify-center gap-1 px-4 text-text-4"
          style={{
            height: 36,
            borderTop: '1px solid var(--color-border)',
          }}
        >
          <span>
            <kbd className="text-text-3">&uarr;&darr;</kbd> navigate
          </span>
          <span className="text-text-4" style={{ margin: '0 6px' }}>
            &middot;
          </span>
          <span>
            <kbd className="text-text-3">&crarr;</kbd> select
          </span>
          <span className="text-text-4" style={{ margin: '0 6px' }}>
            &middot;
          </span>
          <span>
            <kbd className="text-text-3">esc</kbd> close
          </span>
        </div>
      </Command>
    </div>
  );
}

export function CommandPalette({ open, onClose, onAction }: Props): React.ReactElement | null {
  if (!open) return null;
  return <CommandPaletteInner onClose={onClose} onAction={onAction} />;
}
