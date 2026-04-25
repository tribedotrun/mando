import React from 'react';
import { X } from 'lucide-react';

export function ImageChip({
  preview,
  name,
  onRemove,
}: {
  preview: string;
  name: string;
  onRemove: () => void;
}): React.ReactElement {
  return (
    <div className="mb-1 flex items-center">
      <button
        onClick={onRemove}
        className="flex items-center gap-1.5 rounded-md bg-secondary/60 px-2 py-0.5 text-caption text-muted-foreground transition-colors hover:bg-secondary"
      >
        <img src={preview} alt="" className="h-4 w-4 rounded-sm object-cover" />
        <span className="max-w-[160px] truncate">{name}</span>
        <X size={10} className="shrink-0 opacity-60" />
      </button>
    </div>
  );
}
