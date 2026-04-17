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

export function CheckboxItem({
  checked,
  depth,
  text,
}: {
  checked: boolean;
  depth: number;
  text: string;
}): React.ReactElement {
  return (
    <div
      className="flex items-start gap-2 py-1 text-body"
      style={{ paddingLeft: `${4 + depth * 16}px` }}
    >
      <span
        className="mt-0.5 inline-block h-3.5 w-3.5 shrink-0 rounded-sm text-center text-label leading-[14px]"
        style={{
          border: '1px solid var(--border)',
          background: checked ? 'var(--foreground)' : 'transparent',
          color: checked ? 'var(--background)' : 'transparent',
        }}
      >
        {checked ? '✓' : ''}
      </span>
      <span className="text-foreground">
        <InlineMarkdown text={text} />
      </span>
    </div>
  );
}

export function BulletItem({ depth, text }: { depth: number; text: string }): React.ReactElement {
  return (
    <div className="flex gap-2 py-1 text-body" style={{ paddingLeft: `${8 + depth * 16}px` }}>
      <span className="text-text-3">&bull;</span>
      <span className="text-foreground">
        <InlineMarkdown text={text} />
      </span>
    </div>
  );
}

export function NumberedItem({
  num,
  depth,
  text,
}: {
  num: string;
  depth: number;
  text: string;
}): React.ReactElement {
  return (
    <div className="flex gap-2 py-1 text-body" style={{ paddingLeft: `${8 + depth * 16}px` }}>
      <span className="w-4 shrink-0 text-right text-text-3">{num}.</span>
      <span className="text-foreground">
        <InlineMarkdown text={text} />
      </span>
    </div>
  );
}

const HEADING_STYLES: Record<number, string> = {
  1: 'mt-4 mb-2 text-subheading text-foreground',
  2: 'mt-4 mb-1.5 text-body font-semibold text-foreground',
  3: 'mt-3 mb-1 text-body font-semibold text-foreground',
  4: 'mt-3 mb-1 text-caption font-semibold text-foreground',
};

export function HeadingBlock({ level, text }: { level: number; text: string }): React.ReactElement {
  return (
    <div className={HEADING_STYLES[level] ?? HEADING_STYLES[4]}>
      <InlineMarkdown text={text} />
    </div>
  );
}

export function DetailsBlock({
  summaryText,
  children,
}: {
  summaryText: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <details className="my-2 rounded border border-border px-3 py-2">
      <summary className="cursor-pointer text-[12px] font-medium text-foreground select-none">
        <InlineMarkdown text={summaryText} />
      </summary>
      <div className="mt-2 text-[12px]">{children}</div>
    </details>
  );
}
