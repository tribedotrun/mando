import type React from 'react';

/** Standard input style for settings panels and forms. */
export const inputStyle: React.CSSProperties = {
  border: '1px solid var(--color-border)',
  background: 'var(--color-surface-2)',
  color: 'var(--color-text-1)',
};

/** Subtle-bordered input style for onboarding and compact inline forms. */
export const inputStyleSubtle: React.CSSProperties = {
  border: '1px solid var(--color-border-subtle)',
  background: 'var(--color-surface-2)',
  color: 'var(--color-text-1)',
};

/** Subtle-bordered input style that also flexes to fill inline space (onboarding). */
export const inputStyleSubtleFlex: React.CSSProperties = {
  ...inputStyleSubtle,
  flex: 1,
};

/** Standard card container for settings sections. */
export const cardStyle: React.CSSProperties = {
  borderRadius: 'var(--radius-panel)',
  border: '1px solid var(--color-border)',
  background: 'var(--color-surface-1)',
  padding: '24px',
};

/** Standard label color for form fields. */
export const labelStyle: React.CSSProperties = { color: 'var(--color-text-3)' };

/** Standard input class string for settings/form inputs. */
export const inputCls =
  'w-full rounded-md px-3 py-2 text-sm placeholder-[var(--color-text-3)] focus:outline-none';

/** Compact input class string for onboarding and tight inline forms. */
export const inputClsCompact =
  'w-full rounded px-2.5 py-1.5 text-xs placeholder-[var(--color-text-3)] focus:outline-none';

/** Standard label class string for form labels. */
export const labelCls = 'mb-1 block text-label';
