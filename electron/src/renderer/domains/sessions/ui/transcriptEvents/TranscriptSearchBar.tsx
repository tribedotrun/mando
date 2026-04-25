import React from 'react';

interface TranscriptSearchBarProps {
  value: string;
  onChange: (next: string) => void;
}

export function TranscriptSearchBar({
  value,
  onChange,
}: TranscriptSearchBarProps): React.ReactElement {
  return (
    <div className="flex items-center gap-2 border-b border-muted/60 px-4 py-2 text-label text-muted-foreground">
      <span className="opacity-70">search</span>
      <input
        value={value}
        placeholder="filter messages"
        onChange={(e) => onChange(e.target.value)}
        className="flex-1 bg-transparent text-foreground placeholder:text-muted-foreground/60 focus:outline-none"
        data-testid="transcript-search-input"
      />
      {value && (
        <button className="opacity-70 hover:opacity-100" onClick={() => onChange('')}>
          clear
        </button>
      )}
    </div>
  );
}
