import React from 'react';
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from '#renderer/global/ui/table';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';

export {
  CheckboxItem,
  BulletItem,
  NumberedItem,
  HeadingBlock,
  DetailsBlock,
} from '#renderer/global/ui/PrMarkdownBlocksParts';

export function MarkdownTable({
  headerCells,
  rows,
}: {
  headerCells: string[];
  rows: string[][];
}): React.ReactElement {
  return (
    <div className="my-2">
      <Table className="text-caption">
        <TableHeader>
          <TableRow>
            {headerCells.map((h, ci) => (
              <TableHead key={ci} className="h-auto px-3 py-1 text-caption font-medium">
                <InlineMarkdown text={h} />
              </TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((row, ri) => (
            <TableRow key={ri}>
              {row.map((cell, ci) => (
                <TableCell key={ci} className="px-3 py-1 text-muted-foreground">
                  <InlineMarkdown text={cell} />
                </TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

export function AdmonitionBlock({
  type,
  bodyLines,
}: {
  type: string;
  bodyLines: string[];
}): React.ReactElement {
  const admonitionColors: Record<string, string> = {
    NOTE: 'var(--muted-foreground)',
    TIP: 'var(--muted-foreground)',
    IMPORTANT: 'var(--muted-foreground)',
    WARNING: 'var(--stale)',
    CAUTION: 'var(--destructive)',
  };
  const color = admonitionColors[type] ?? 'var(--border)';

  return (
    <div
      className="my-2 rounded px-3 py-2 text-body [overflow-wrap:anywhere]"
      style={{
        background: `color-mix(in srgb, ${color} 8%, transparent)`,
        border: `1px solid color-mix(in srgb, ${color} 25%, transparent)`,
      }}
    >
      <div className="mb-1 text-label" style={{ color }}>
        {type}
      </div>
      {bodyLines.map((bl, bi) => (
        <div key={bi} className="text-muted-foreground">
          <InlineMarkdown text={bl} />
        </div>
      ))}
    </div>
  );
}

export function BlockquoteBlock({ lines }: { lines: string[] }): React.ReactElement {
  return (
    <div className="my-1 border-l-2 border-muted-foreground/30 pl-3 text-body italic text-text-3">
      {lines.map((bl, bi) => (
        <div key={bi}>
          <InlineMarkdown text={bl} />
        </div>
      ))}
    </div>
  );
}
