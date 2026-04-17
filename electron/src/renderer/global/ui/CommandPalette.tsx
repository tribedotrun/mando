import React, { useRef } from 'react';
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
} from 'lucide-react';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import {
  Command,
  CommandInput,
  CommandList,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandShortcut,
  CommandSeparator,
} from '#renderer/global/ui/command';
import { Kbd, KbdGroup } from '#renderer/global/ui/kbd';

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

function ShortcutKeys({ shortcut }: { shortcut: string }): React.ReactElement {
  const parts = shortcut.split(/\s+/);
  return (
    <CommandShortcut>
      <KbdGroup>
        {parts.map((part, i) => (
          <Kbd key={i}>{part}</Kbd>
        ))}
      </KbdGroup>
    </CommandShortcut>
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
    <CommandItem value={cmd.name} onSelect={onSelect}>
      <span className="shrink-0 text-muted-foreground">{cmd.icon}</span>
      <span className="flex-1">{cmd.name}</span>
      {cmd.shortcut && <ShortcutKeys shortcut={cmd.shortcut} />}
    </CommandItem>
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
      className="fixed inset-0 z-[300] flex items-start justify-center bg-overlay pt-[20vh]"
      data-command-palette
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <Command
        shouldFilter={true}
        className="w-[480px] !h-auto max-h-[60vh] rounded-lg bg-popover shadow-lg"
        onKeyDown={(e: React.KeyboardEvent) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            onClose();
          }
        }}
      >
        <CommandInput ref={inputRef} placeholder="Type a command..." />

        <CommandList className="max-h-[50vh]">
          <CommandEmpty>No commands found</CommandEmpty>

          <CommandGroup heading="Recent">
            {RECENT_COMMANDS.map((cmd) => (
              <CommandRow key={cmd.id} cmd={cmd} onSelect={() => handleSelect(cmd.id)} />
            ))}
          </CommandGroup>

          <CommandSeparator />

          <CommandGroup heading="Navigation">
            {NAVIGATION_COMMANDS.map((cmd) => (
              <CommandRow key={cmd.id} cmd={cmd} onSelect={() => handleSelect(cmd.id)} />
            ))}
          </CommandGroup>

          <CommandSeparator />

          <CommandGroup heading="Actions">
            {ACTION_COMMANDS.map((cmd) => (
              <CommandRow key={cmd.id} cmd={cmd} onSelect={() => handleSelect(cmd.id)} />
            ))}
          </CommandGroup>
        </CommandList>

        {/* Footer */}
        <div className="flex items-center justify-center gap-3 px-4 py-2 text-xs text-muted-foreground">
          <span className="flex items-center gap-1">
            <Kbd>&uarr;&darr;</Kbd> navigate
          </span>
          <span className="text-text-4">&middot;</span>
          <span className="flex items-center gap-1">
            <Kbd>&crarr;</Kbd> select
          </span>
          <span className="text-text-4">&middot;</span>
          <span className="flex items-center gap-1">
            <Kbd>esc</Kbd> close
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
