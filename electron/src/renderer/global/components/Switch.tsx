import React from 'react';
import * as SwitchPrimitive from '@radix-ui/react-switch';
import { cn } from '#renderer/cn';

interface SwitchProps {
  testId?: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  opacity?: number;
  className?: string;
}

export function Switch({
  testId,
  checked,
  onCheckedChange,
  disabled,
  opacity,
  className,
}: SwitchProps): React.ReactElement {
  return (
    <SwitchPrimitive.Root
      data-testid={testId}
      checked={checked}
      onCheckedChange={onCheckedChange}
      disabled={disabled}
      className={cn(
        'relative shrink-0 rounded-full border-none transition-colors',
        'data-[state=checked]:bg-accent data-[state=unchecked]:bg-surface-3',
        disabled ? 'cursor-default' : 'cursor-pointer',
        className,
      )}
      style={{
        width: 36,
        height: 20,
        opacity: opacity ?? (disabled ? 0.5 : 1),
      }}
    >
      <SwitchPrimitive.Thumb
        className="pointer-events-none absolute rounded-full bg-text-1 shadow transition-[left] data-[state=checked]:left-[18px] data-[state=unchecked]:left-0.5"
        style={{ width: 16, height: 16, top: 2 }}
      />
    </SwitchPrimitive.Root>
  );
}
