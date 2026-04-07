import React from 'react';
import { Card, CardHeader, CardTitle, CardContent, CardFooter } from '#renderer/components/ui/card';
import { Kbd } from '#renderer/components/ui/kbd';
import { Separator } from '#renderer/components/ui/separator';
import { Button } from '#renderer/components/ui/button';

interface ShortcutEntry {
  keys: string[];
  label: string;
}

const GENERAL: ShortcutEntry[] = [
  { keys: ['\u2318', 'K'], label: 'Command palette' },
  { keys: ['\u2318', ','], label: 'Settings' },
  { keys: ['\u2318', 'N'], label: 'New task' },
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
  { keys: ['\u2318', 'Enter'], label: 'Submit form' },
];

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
      <Card
        className="w-[640px] max-w-[90vw] gap-0 rounded-lg border py-0 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <CardHeader className="px-5 py-3">
          <div className="flex items-center justify-between">
            <CardTitle className="text-sm">Keyboard shortcuts</CardTitle>
            <Button
              variant="ghost"
              size="icon-xs"
              onClick={onClose}
              className="text-lg leading-none text-muted-foreground hover:text-foreground"
            >
              &times;
            </Button>
          </div>
        </CardHeader>

        <Separator />

        <CardContent className="grid grid-cols-3 gap-6 px-5 py-4">
          <ShortcutColumn title="General" entries={GENERAL} />
          <ShortcutColumn title="Navigation" entries={NAVIGATION} />
          <ShortcutColumn title="Actions" entries={ACTIONS} />
        </CardContent>

        <Separator />

        <CardFooter className="px-5 py-2.5 text-xs text-text-4">
          Press <Kbd className="mx-1">Esc</Kbd> to close
        </CardFooter>
      </Card>
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
      <div className="text-label mb-2 text-text-4">{title}</div>
      <div className="space-y-1.5">
        {entries.map((entry, i) => (
          <div key={i} className="flex items-center justify-between gap-2">
            <span className="text-caption text-muted-foreground">{entry.label}</span>
            <span className="flex shrink-0 items-center gap-1">
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
