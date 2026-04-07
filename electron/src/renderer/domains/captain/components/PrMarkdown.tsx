import React, { useRef, useState } from 'react';
import { ImageLightbox } from '#renderer/domains/captain/components/ImageLightbox';

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
        // Lone ``` with no closing — treat as regular text
        elements.push(
          <div key={elements.length} className="py-1 text-[12px] text-text-1">
            {line}
          </div>,
        );
        continue;
      }
      elements.push(
        <pre
          key={elements.length}
          className="my-2 overflow-x-auto rounded bg-surface-2 p-3 text-[11px] leading-relaxed text-text-1"
          style={{
            border: '1px solid var(--color-border-subtle)',
            fontFamily: 'var(--font-mono)',
          }}
        >
          {codeLines.join('\n')}
        </pre>,
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
        <div key={elements.length} className="my-2 overflow-x-auto">
          <table className="w-full text-caption" style={{ borderCollapse: 'collapse' }}>
            <thead>
              <tr style={{ borderBottom: '1px solid var(--color-border)' }}>
                {headerCells.map((h, ci) => (
                  <th key={ci} className="px-3 py-1 text-left font-medium text-text-1">
                    {renderInline(h)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, ri) => (
                <tr key={ri} style={{ borderBottom: '1px solid var(--color-border-subtle)' }}>
                  {row.map((cell, ci) => (
                    <td key={ci} className="px-3 py-1 text-text-2">
                      {renderInline(cell)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>,
      );
      continue;
    }

    // Horizontal rule
    if (/^---+$|^\*\*\*+$|^___+$/.test(line.trim())) {
      elements.push(
        <hr
          key={elements.length}
          className="my-3"
          style={{ border: 'none', borderTop: '1px solid var(--color-border-subtle)' }}
        />,
      );
      i++;
      continue;
    }

    // Headings
    if (line.startsWith('#### ')) {
      elements.push(
        <div key={elements.length} className="mt-3 mb-1 text-[12px] font-semibold text-text-1">
          {renderInline(line.slice(5))}
        </div>,
      );
      i++;
      continue;
    }
    if (line.startsWith('### ')) {
      elements.push(
        <div key={elements.length} className="mt-3 mb-1 text-[13px] font-semibold text-text-1">
          {renderInline(line.slice(4))}
        </div>,
      );
      i++;
      continue;
    }
    if (line.startsWith('## ')) {
      elements.push(
        <div key={elements.length} className="mt-4 mb-1.5 text-body font-semibold text-text-1">
          {renderInline(line.slice(3))}
        </div>,
      );
      i++;
      continue;
    }
    if (line.startsWith('# ')) {
      elements.push(
        <div key={elements.length} className="mt-4 mb-2 text-subheading text-text-1">
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
          NOTE: 'var(--color-accent)',
          TIP: 'var(--color-success)',
          IMPORTANT: 'var(--color-accent)',
          WARNING: 'var(--color-stale)',
          CAUTION: 'var(--color-error)',
        };
        const color = admonitionColors[type] ?? 'var(--color-border)';
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
              <div key={bi} className="text-text-2">
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
          className="my-1 pl-3 text-[12px] italic text-text-3"
          style={{
            borderLeft: '2px solid var(--color-border)',
          }}
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
              border: '1px solid var(--color-border)',
              background: checked ? 'var(--color-accent)' : 'transparent',
              color: checked ? 'var(--color-bg)' : 'transparent',
            }}
          >
            {checked ? '✓' : ''}
          </span>
          <span className="text-text-1">{renderInline(checkMatch[3])}</span>
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
          <span className="text-text-1">{renderInline(ulMatch[2])}</span>
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
          <span className="text-text-1">{renderInline(olMatch[2])}</span>
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
        className="break-words py-1 text-[12px] leading-relaxed text-text-1"
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
            className="my-1 max-w-full cursor-pointer rounded transition-opacity hover:opacity-80"
            style={{ maxHeight: 300, border: '1px solid var(--color-border-subtle)' }}
          />,
        );
      } else {
        parts.push(
          <span key={key++} className="text-text-3" style={{ fontStyle: 'italic' }}>
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
            className="text-accent hover:underline"
          >
            {match[3]}
          </a>
        ) : (
          <span key={key++} className="text-accent">
            {match[3]}
          </span>
        ),
      );
    } else if (match[5]) {
      // Bold
      parts.push(
        <strong key={key++} className="text-text-1" style={{ fontWeight: 600 }}>
          {match[5]}
        </strong>,
      );
    } else if (match[6]) {
      // Italic
      parts.push(<em key={key++}>{match[6]}</em>);
    } else if (match[7]) {
      // Inline code
      parts.push(
        <code
          key={key++}
          className="rounded bg-surface-3 px-1 py-1 text-[11px]"
          style={{ fontFamily: 'var(--font-mono)' }}
        >
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
            className="text-accent hover:underline"
          >
            {match[12]}
          </a>
        ) : (
          <span key={key++} className="text-accent">
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
            className="my-1 max-w-full cursor-pointer rounded transition-opacity hover:opacity-80"
            style={{ maxHeight: 300, border: '1px solid var(--color-border-subtle)' }}
          />
        ) : (
          <span key={key++} className="text-text-3" style={{ fontStyle: 'italic' }}>
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
