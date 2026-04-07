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

export function PrMarkdown({ text }: { text: string }): React.ReactElement {
  const [lightbox, setLightbox] = useState<{ images: string[]; index: number } | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

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
          i++; // skip closing ```
          break;
        }
        codeLines.push(lines[i]);
        i++;
      }
      if (!foundClose && codeLines.length === 0) {
        // Lone ``` with no closing -- treat as regular text
        elements.push(
          <div key={elements.length} className="py-1 text-[12px] text-foreground">
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
        <div key={elements.length} className="mt-3 mb-1 text-[12px] font-semibold text-foreground">
          {renderInline(line.slice(5))}
        </div>,
      );
      i++;
      continue;
    }
    if (line.startsWith('### ')) {
      elements.push(
        <div key={elements.length} className="mt-3 mb-1 text-[13px] font-semibold text-foreground">
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
          NOTE: 'var(--primary)',
          TIP: 'var(--success)',
          IMPORTANT: 'var(--primary)',
          WARNING: 'var(--stale)',
          CAUTION: 'var(--destructive)',
        };
        const color = admonitionColors[type] ?? 'var(--border)';
        // Collect continuation lines
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
            className="my-2 rounded px-3 py-2 text-[12px]"
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
      // Collect consecutive blockquote lines
      const bqLines = [content];
      i++;
      while (i < lines.length && lines[i].startsWith('> ')) {
        bqLines.push(lines[i].slice(2));
        i++;
      }
      elements.push(
        <div
          key={elements.length}
          className="my-1 border-l-2 border-muted-foreground/30 pl-3 text-[12px] italic text-text-3"
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
      elements.push(
        <div key={elements.length} className="flex items-start gap-2 py-1 pl-1 text-[12px]">
          <span
            className="mt-0.5 inline-block h-3.5 w-3.5 shrink-0 rounded-sm text-center text-label leading-[14px]"
            style={{
              border: '1px solid var(--border)',
              background: checked ? 'var(--primary)' : 'transparent',
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
      elements.push(
        <div key={elements.length} className="flex gap-2 py-1 pl-2 text-[12px]">
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
      elements.push(
        <div key={elements.length} className="flex gap-2 py-1 pl-2 text-[12px]">
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
      <div
        key={elements.length}
        className="break-words py-1 text-[12px] leading-relaxed text-foreground"
      >
        {renderInline(line)}
      </div>,
    );
    i++;
  }

  const handleClick = (e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.tagName !== 'IMG' || !target.getAttribute('data-lightbox-src')) return;
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

/** Render inline markdown: **bold**, *italic*, `code`, [links](url), ![images](url), HTML tags. */
function renderInline(text: string): React.ReactNode {
  // Strip remaining inline HTML comments
  const cleaned = text.replace(/<!--.*?-->/g, '');
  const parts: React.ReactNode[] = [];
  // Match: images, links, bold, italic, inline code, strikethrough, HTML tags
  const regex =
    /!\[([^\]]*)\]\(([^)]+)\)|\[([^\]]*)\]\(([^)]+)\)|\*\*(.+?)\*\*|\*(.+?)\*|`(.+?)`|~~(.+?)~~|<(sup|sub|strong|em|b|i)>(.*?)<\/\9>|<a\s+href="([^"]*)"[^>]*>(.*?)<\/a>|<img\s+[^>]*src="([^"]*)"[^>]*\/?>/g;
  let last = 0;
  let key = 0;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(cleaned)) !== null) {
    if (match.index > last) {
      parts.push(cleaned.slice(last, match.index));
    }
    if (match[1] !== undefined) {
      // Image: ![alt](url) — only allow https:// to prevent resource loading exploits
      const imgUrl = match[2] ?? '';
      if (imgUrl.startsWith('https://')) {
        parts.push(
          <img
            key={key++}
            src={imgUrl}
            alt={match[1]}
            data-lightbox-src={imgUrl}
            className="my-1 max-h-[300px] max-w-full cursor-pointer rounded transition-opacity hover:opacity-80"
          />,
        );
      } else {
        parts.push(
          <span key={key++} className="italic text-text-3">
            [{match[1] || 'image'}]
          </span>,
        );
      }
    } else if (match[3] !== undefined) {
      // Link: [text](url) — only allow https:// to prevent local navigation exploits
      const linkUrl = match[4] ?? '';
      const isSafe = linkUrl.startsWith('https://');
      parts.push(
        isSafe ? (
          <a
            key={key++}
            href={linkUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="text-primary hover:underline"
          >
            {match[3]}
          </a>
        ) : (
          <span key={key++} className="text-primary">
            {match[3]}
          </span>
        ),
      );
    } else if (match[5]) {
      // Bold
      parts.push(
        <strong key={key++} className="font-semibold text-foreground">
          {match[5]}
        </strong>,
      );
    } else if (match[6]) {
      // Italic
      parts.push(<em key={key++}>{match[6]}</em>);
    } else if (match[7]) {
      // Inline code
      parts.push(
        <code key={key++} className="rounded bg-secondary px-1 py-1 font-mono text-[11px]">
          {match[7]}
        </code>,
      );
    } else if (match[8]) {
      // Strikethrough
      parts.push(
        <del key={key++} className="text-text-3">
          {match[8]}
        </del>,
      );
    } else if (match[9]) {
      // HTML inline tags: <sup>, <sub>, <strong>, <em>, <b>, <i>
      const tag = match[9];
      const content = match[10];
      const Tag = ({ sup: 'sup', sub: 'sub', strong: 'strong', em: 'em', b: 'strong', i: 'em' }[
        tag
      ] ?? 'span') as keyof React.JSX.IntrinsicElements;
      parts.push(<Tag key={key++}>{content}</Tag>);
    } else if (match[11] !== undefined) {
      // HTML <a> tag — same https-only sanitization as markdown links
      const aHref = match[11] ?? '';
      const aIsSafe = aHref.startsWith('https://');
      parts.push(
        aIsSafe ? (
          <a
            key={key++}
            href={aHref}
            target="_blank"
            rel="noopener noreferrer"
            className="text-primary hover:underline"
          >
            {match[12]}
          </a>
        ) : (
          <span key={key++} className="text-primary">
            {match[12]}
          </span>
        ),
      );
    } else if (match[13] !== undefined) {
      // HTML <img> tag — same https-only sanitization as markdown images
      const imgSrc = match[13] ?? '';
      parts.push(
        imgSrc.startsWith('https://') ? (
          <img
            key={key++}
            src={imgSrc}
            alt=""
            data-lightbox-src={imgSrc}
            className="my-1 max-h-[300px] max-w-full cursor-pointer rounded transition-opacity hover:opacity-80"
          />
        ) : (
          <span key={key++} className="italic text-text-3">
            [image]
          </span>
        ),
      );
    }
    last = match.index + match[0].length;
  }
  if (last < cleaned.length) {
    parts.push(cleaned.slice(last));
  }
  return parts.length === 1 ? parts[0] : <>{parts}</>;
}
