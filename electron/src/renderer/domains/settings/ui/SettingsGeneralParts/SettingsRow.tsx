import React from 'react';

export function SettingsRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <div className="flex min-h-[40px] items-center justify-between py-2.5">
      <span className="text-body text-foreground">{label}</span>
      <div className="flex items-center">{children}</div>
    </div>
  );
}
