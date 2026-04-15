import React, { useRef, useState } from 'react';
import { ImageLightbox } from '#renderer/domains/captain/components/ImageLightbox';
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from '#renderer/components/ui/table';
import { CodeBlock } from '#renderer/components/ui/code-block';
import { Separator } from '#renderer/components/ui/separator';
import { indentDepth, renderInline } from '#renderer/domains/captain/components/pr-markdown-inline';

export function PrMarkdown({ text }: { text: string }): React.ReactElement {
  const [lightbox, setLightbox] = useState<{ images: string[]; index: number } | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const elements = parseBlocks(text);

  const handleClick = (e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.tagName !== 'IMG' || !target.getAttribute('data-lightbox-src')) return;
    e.stopPropagation();
    const el = containerRef.current;
    if (!el) return;
    const imgs = Array.from(el.querySelectorAll<HTMLImageElement>('img[data-lightbox-src]'));
    const urls = imgs.map((img) => img.src);
    const idx = imgs.indexOf(target as HTMLImageElement);
    if (idx !== -1 && urls.length > 0) setLightbox({ images: urls, index: idx });
  };

  return (
    <div ref={containerRef} onClick={handleClick}>
      {elements}
      {lightbox && (
        <ImageLightbox
          images={lightbox.images}
          index={lightbox.index}
          onClose={() => setLightbox(null)}
          onNavigate={(i) => setLightbox((prev) => (prev ? { ...prev, index: i } : null))}
        />
      )}
    </div>
  );
}

/** Parse markdown text into block-level React elements. */
function parseBlocks(text: string): React.ReactNode[] {
  // Pre-process: strip HTML comments and clean up
  const cleaned = text
    .replace(/<!--[\s\S]*?-->/g, '') // strip HTML comments
    .replace(/\n{3,}/g, '\n\n'); // collapse excessive blank lines
  const lines = cleaned.split('\n');
  const elements: React.ReactNode[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Fenced code block
    if (line.trimStart().startsWith('```')) {
      const langTag = line.trimStart().slice(3).trim();
      const codeLines: string[] = [];
      i++;
      let foundClose = false;
      while (i < lines.length) {
        if (lines[i].trimStart().startsWith('```')) {
          foundClose = true;
          i++;
          break;
        }
        codeLines.push(lines[i]);
        i++;
      }
      if (!foundClose && codeLines.length === 0) {
        elements.push(
          <div key={elements.length} className="py-1 text-body text-foreground">
            {line}
          </div>,
        );
        continue;
      }
      elements.push(
        <CodeBlock
          key={elements.length}
          code={codeLines.join('\n')}
          language={langTag || 'text'}
        />,
      );
      continue;
    }

    // HTML <details>/<summary> block
    if (line.trimStart().startsWith('<details')) {
      const detailLines: string[] = [line];
      i++;
      let depth = 1;
      // Opening line may already contain </details> (e.g. single-line or after comment stripping)
      if (line.trimStart().includes('</details>')) depth--;
      while (i < lines.length && depth > 0) {
        const cur = lines[i];
        if (cur.trimStart().startsWith('<details')) depth++;
        if (cur.trimStart().includes('</details>') && depth > 0) depth--;
        detailLines.push(cur);
        i++;
      }
      // Extract summary text
      const joined = detailLines.join('\n');
      const summaryMatch = joined.match(/<summary>([\s\S]*?)<\/summary>/);
      const summaryText = summaryMatch ? summaryMatch[1].trim() : 'Details';
      // Extract body content between </summary> and </details>
      const afterSummary = joined
        .replace(/<details[^>]*>/, '')
        .replace(/<summary>[\s\S]*?<\/summary>/, '')
        .replace(/<\/details>\s*$/, '')
        .trim();
      elements.push(
        <details key={elements.length} className="my-2 rounded border border-border px-3 py-2">
          <summary className="cursor-pointer text-[12px] font-medium text-foreground select-none">
            {renderInline(summaryText)}
          </summary>
          <div className="mt-2 text-[12px]">
            <PrMarkdown text={afterSummary} />
          </div>
        </details>,
      );
      continue;
    }

    // Markdown table
    if (
      line.trim().startsWith('|') &&
      i + 1 < lines.length &&
      /^\|[\s:|-]+\|$/.test(lines[i + 1].trim())
    ) {
      const tableLines: string[] = [];
      while (i < lines.length && lines[i].trim().startsWith('|')) {
        tableLines.push(lines[i]);
        i++;
      }
      const headerCells = tableLines[0]
        .split('|')
        .filter(Boolean)
        .map((c) => c.trim());
      const rows = tableLines.slice(2).map((row) =>
        row
          .split('|')
          .filter(Boolean)
          .map((c) => c.trim()),
      );
      elements.push(
        <div key={elements.length} className="my-2">
          <Table className="text-caption">
            <TableHeader>
              <TableRow>
                {headerCells.map((h, ci) => (
                  <TableHead key={ci} className="h-auto px-3 py-1 text-caption font-medium">
                    {renderInline(h)}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((row, ri) => (
                <TableRow key={ri}>
                  {row.map((cell, ci) => (
                    <TableCell key={ci} className="px-3 py-1 text-muted-foreground">
                      {renderInline(cell)}
                    </TableCell>
                  ))}
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>,
      );
      continue;
    }

    // Horizontal rule
    if (/^---+$|^\*\*\*+$|^___+$/.test(line.trim())) {
      elements.push(<Separator key={elements.length} className="my-3" />);
      i++;
      continue;
    }

    // Headings
    if (line.startsWith('#### ')) {
      elements.push(
        <div key={elements.length} className="mt-3 mb-1 text-caption font-semibold text-foreground">
          {renderInline(line.slice(5))}
        </div>,
      );
      i++;
      continue;
    }
    if (line.startsWith('### ')) {
      elements.push(
        <div key={elements.length} className="mt-3 mb-1 text-body font-semibold text-foreground">
          {renderInline(line.slice(4))}
        </div>,
      );
      i++;
      continue;
    }
    if (line.startsWith('## ')) {
      elements.push(
        <div key={elements.length} className="mt-4 mb-1.5 text-body font-semibold text-foreground">
          {renderInline(line.slice(3))}
        </div>,
      );
      i++;
      continue;
    }
    if (line.startsWith('# ')) {
      elements.push(
        <div key={elements.length} className="mt-4 mb-2 text-subheading text-foreground">
          {renderInline(line.slice(2))}
        </div>,
      );
      i++;
      continue;
    }

    // Blockquote (including GitHub admonitions)
    if (line.startsWith('> ')) {
      const content = line.slice(2);
      const admonition = content.match(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]/);
      if (admonition) {
        const type = admonition[1];
        const admonitionColors: Record<string, string> = {
          NOTE: 'var(--muted-foreground)',
          TIP: 'var(--muted-foreground)',
          IMPORTANT: 'var(--muted-foreground)',
          WARNING: 'var(--stale)',
          CAUTION: 'var(--destructive)',
        };
        const color = admonitionColors[type] ?? 'var(--border)';
        const bodyLines: string[] = [];
        const afterTag = content.slice(admonition[0].length).trim();
        if (afterTag) bodyLines.push(afterTag);
        i++;
        while (i < lines.length && lines[i].startsWith('> ')) {
          bodyLines.push(lines[i].slice(2));
          i++;
        }
        elements.push(
          <div
            key={elements.length}
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
                {renderInline(bl)}
              </div>
            ))}
          </div>,
        );
        continue;
      }
      const bqLines = [content];
      i++;
      while (i < lines.length && lines[i].startsWith('> ')) {
        bqLines.push(lines[i].slice(2));
        i++;
      }
      elements.push(
        <div
          key={elements.length}
          className="my-1 border-l-2 border-muted-foreground/30 pl-3 text-body italic text-text-3"
        >
          {bqLines.map((bl, bi) => (
            <div key={bi}>{renderInline(bl)}</div>
          ))}
        </div>,
      );
      continue;
    }

    // Checkbox list item
    const checkMatch = line.match(/^(\s*)[-*]\s+\[([ xX])\]\s+(.*)/);
    if (checkMatch) {
      const checked = checkMatch[2] !== ' ';
      const depth = indentDepth(checkMatch[1]);
      elements.push(
        <div
          key={elements.length}
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
          <span className="text-foreground">{renderInline(checkMatch[3])}</span>
        </div>,
      );
      i++;
      continue;
    }

    // Unordered list
    const ulMatch = line.match(/^(\s*)[-*]\s+(.*)/);
    if (ulMatch) {
      const depth = indentDepth(ulMatch[1]);
      elements.push(
        <div
          key={elements.length}
          className="flex gap-2 py-1 text-body"
          style={{ paddingLeft: `${8 + depth * 16}px` }}
        >
          <span className="text-text-3">&bull;</span>
          <span className="text-foreground">{renderInline(ulMatch[2])}</span>
        </div>,
      );
      i++;
      continue;
    }

    // Ordered list
    const olMatch = line.match(/^(\s*)\d+\.\s+(.*)/);
    if (olMatch) {
      const num = line.match(/^(\s*)(\d+)\./)?.[2] ?? '1';
      const depth = indentDepth(olMatch[1]);
      elements.push(
        <div
          key={elements.length}
          className="flex gap-2 py-1 text-body"
          style={{ paddingLeft: `${8 + depth * 16}px` }}
        >
          <span className="w-4 shrink-0 text-right text-text-3">{num}.</span>
          <span className="text-foreground">{renderInline(olMatch[2])}</span>
        </div>,
      );
      i++;
      continue;
    }

    // Empty line
    if (line.trim() === '') {
      elements.push(<div key={elements.length} className="h-2" />);
      i++;
      continue;
    }

    // Regular paragraph
    elements.push(
      <div key={elements.length} className="break-words py-1 text-body text-foreground">
        {renderInline(line)}
      </div>,
    );
    i++;
  }

  return elements;
}
