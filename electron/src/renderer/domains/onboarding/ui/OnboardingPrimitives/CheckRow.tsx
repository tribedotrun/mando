import React from 'react';

export function CheckRow({ ok, label }: { ok: boolean; label: string }): React.ReactElement {
  return (
    <div className="flex items-center gap-2">
      <span className={`text-[13px] ${ok ? 'text-success' : 'text-destructive'}`}>
        {ok ? '✓' : '✗'}
      </span>
      <span className={`text-body ${ok ? 'text-foreground' : 'text-destructive'}`}>{label}</span>
    </div>
  );
}
