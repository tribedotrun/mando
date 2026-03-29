import React from 'react';

/** Render basic markdown: **bold**, *italic*, `code`, ## headings, - lists. */
export function MarkdownText({ text }: { text: string }): React.ReactElement {
  const lines = text.split('\n');
  const elements: React.ReactNode[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Fenced code block: ```
    if (line.trimStart().startsWith('```')) {
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith('```')) {
        codeLines.push(lines[i]);
        i++;
      }
      if (i < lines.length) i++; // skip closing ``` (guard unclosed fences)
      elements.push(
        <pre
          key={elements.length}
          className="my-1 overflow-x-auto rounded px-3 py-2 text-[11px]"
          style={{ background: 'var(--color-surface-3)', fontFamily: 'var(--font-mono)' }}
        >
          {codeLines.join('\n')}
        </pre>,
      );
    } else if (line.startsWith('### ')) {
      elements.push(
        <div
          key={elements.length}
          className="mt-2 mb-0.5 text-[12px] font-semibold"
          style={{ color: 'var(--color-text-1)' }}
        >
          {renderInline(line.slice(4))}
        </div>,
      );
      i++;
    } else if (line.startsWith('## ')) {
      elements.push(
        <div
          key={elements.length}
          className="mt-3 mb-1 text-body font-semibold"
          style={{ color: 'var(--color-text-1)' }}
        >
          {renderInline(line.slice(3))}
        </div>,
      );
      i++;
    } else if (line.startsWith('- ')) {
      elements.push(
        <div key={elements.length} className="flex gap-1.5 pl-2">
          <span style={{ color: 'var(--color-text-3)' }}>&bull;</span>
          <span>{renderInline(line.slice(2))}</span>
        </div>,
      );
      i++;
    } else if (line.trim() === '') {
      elements.push(<div key={elements.length} className="h-2" />);
      i++;
    } else {
      elements.push(<div key={elements.length}>{renderInline(line)}</div>);
      i++;
    }
  }

  return <>{elements}</>;
}

/** Render inline markdown: **bold**, *italic*, `code`. */
function renderInline(text: string): React.ReactNode {
  const parts: React.ReactNode[] = [];
  // Match **bold**, *italic*, `code`
  const regex = /(\*\*(.+?)\*\*|\*(.+?)\*|`(.+?)`)/g;
  let last = 0;
  let match: RegExpExecArray | null;
  let key = 0;

  while ((match = regex.exec(text)) !== null) {
    if (match.index > last) {
      parts.push(text.slice(last, match.index));
    }
    if (match[2]) {
      parts.push(
        <strong key={key++} style={{ fontWeight: 600 }}>
          {match[2]}
        </strong>,
      );
    } else if (match[3]) {
      parts.push(
        <em key={key++} style={{ fontStyle: 'italic' }}>
          {match[3]}
        </em>,
      );
    } else if (match[4]) {
      parts.push(
        <code
          key={key++}
          className="rounded px-1 py-0.5 text-[11px]"
          style={{ background: 'var(--color-surface-3)', fontFamily: 'var(--font-mono)' }}
        >
          {match[4]}
        </code>,
      );
    }
    last = match.index + match[0].length;
  }
  if (last < text.length) {
    parts.push(text.slice(last));
  }
  return parts.length === 1 ? parts[0] : <>{parts}</>;
}
