import React from 'react';

export function DetailSection({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <div className="mb-5">
      <div className="mb-2 text-label text-text-4">{label}</div>
      {children}
    </div>
  );
}
