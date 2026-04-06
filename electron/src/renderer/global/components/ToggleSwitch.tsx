import React from 'react';

interface Props {
  testId?: string;
  checked: boolean;
  onChange: () => void;
  disabled?: boolean;
  opacity?: number;
}

export function ToggleSwitch({
  testId,
  checked,
  onChange,
  disabled,
  opacity,
}: Props): React.ReactElement {
  return (
    <button
      data-testid={testId}
      onClick={onChange}
      disabled={disabled}
      className="relative shrink-0 rounded-full transition-colors"
      style={{
        width: 36,
        height: 20,
        background: checked ? 'var(--color-accent)' : 'var(--color-surface-3)',
        border: 'none',
        cursor: disabled ? 'default' : 'pointer',
        opacity: opacity ?? (disabled ? 0.5 : 1),
      }}
      role="switch"
      aria-checked={checked}
    >
      <span
        className="pointer-events-none absolute rounded-full bg-[var(--color-text-1)] shadow transition-[left]"
        style={{ width: 16, height: 16, top: 2, left: checked ? 18 : 2 }}
      />
    </button>
  );
}
