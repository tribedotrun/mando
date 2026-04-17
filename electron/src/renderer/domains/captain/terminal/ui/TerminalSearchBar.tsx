import { Search, ChevronUp, ChevronDown, X } from 'lucide-react';
import { Button } from '#renderer/global/ui/button';
import { Input } from '#renderer/global/ui/input';

interface TerminalSearchBarProps {
  query: string;
  resultCount: number;
  resultIndex: number;
  onChange: (value: string) => void;
  onNext: () => void;
  onPrevious: () => void;
  onClose: () => void;
}

export function TerminalSearchBar({
  query,
  resultCount,
  resultIndex,
  onChange,
  onNext,
  onPrevious,
  onClose,
}: TerminalSearchBarProps): React.ReactElement {
  const label = resultCount > 0 ? `${resultIndex + 1}/${resultCount}` : query ? '0/0' : 'Find';

  return (
    <div className="absolute top-3 right-3 z-20 flex items-center gap-2 rounded-lg border border-border bg-card/95 p-2 shadow-lg backdrop-blur">
      <Search size={14} className="text-text-3" />
      <Input
        autoFocus
        value={query}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault();
            if (event.shiftKey) onPrevious();
            else onNext();
          }
          if (event.key === 'Escape') {
            event.preventDefault();
            onClose();
          }
        }}
        placeholder="Find in terminal"
        className="h-8 w-56 bg-transparent text-sm"
      />
      <span className="min-w-10 text-right text-xs text-text-3">{label}</span>
      <Button variant="ghost" size="icon-xs" onClick={onPrevious} aria-label="Previous match">
        <ChevronUp size={14} />
      </Button>
      <Button variant="ghost" size="icon-xs" onClick={onNext} aria-label="Next match">
        <ChevronDown size={14} />
      </Button>
      <Button variant="ghost" size="icon-xs" onClick={onClose} aria-label="Close search">
        <X size={14} />
      </Button>
    </div>
  );
}
