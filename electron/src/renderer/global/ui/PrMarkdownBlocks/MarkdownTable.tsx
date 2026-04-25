import React from 'react';
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from '#renderer/global/ui/primitives/table';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';

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
