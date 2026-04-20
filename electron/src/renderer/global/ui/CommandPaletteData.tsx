import React from 'react';
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
import { CommandItem, CommandShortcut } from '#renderer/global/ui/command';
import { Kbd, KbdGroup } from '#renderer/global/ui/kbd';

export interface CommandDef {
  id: string;
  name: string;
  shortcut?: string;
  section: 'recent' | 'navigation' | 'actions';
  icon: React.ReactNode;
}

/* -- Command definitions -- */

export const RECENT_COMMANDS: CommandDef[] = [
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

export const NAVIGATION_COMMANDS: CommandDef[] = [
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

export const ACTION_COMMANDS: CommandDef[] = [
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

export function CommandRow({
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
