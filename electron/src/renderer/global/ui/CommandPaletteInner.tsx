import React, { useRef } from 'react';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import {
  Command,
  CommandInput,
  CommandList,
  CommandEmpty,
  CommandGroup,
  CommandSeparator,
} from '#renderer/global/ui/primitives/command';
import { Kbd } from '#renderer/global/ui/primitives/kbd';
import {
  RECENT_COMMANDS,
  NAVIGATION_COMMANDS,
  ACTION_COMMANDS,
  CommandRow,
} from '#renderer/global/ui/CommandRow';

/* -- Inner component -- mounted only when open -- */

export function CommandPaletteInner({
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
