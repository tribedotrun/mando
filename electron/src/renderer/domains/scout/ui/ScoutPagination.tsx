import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';

interface Props {
  page: number;
  pages: number;
  onPageChange: (page: number) => void;
}

export function ScoutPagination({ page, pages, onPageChange }: Props): React.ReactElement | null {
  if (pages <= 1) return null;

  return (
    <div className="flex items-center justify-center gap-2 pt-2">
      <Button
        variant="outline"
        size="sm"
        onClick={() => onPageChange(page - 1)}
        disabled={page === 0}
      >
        Prev
      </Button>
      <span className="text-code tabular-nums text-muted-foreground">
        {page + 1} / {pages}
      </span>
      <Button
        variant="outline"
        size="sm"
        onClick={() => onPageChange(page + 1)}
        disabled={page >= pages - 1}
      >
        Next
      </Button>
    </div>
  );
}
