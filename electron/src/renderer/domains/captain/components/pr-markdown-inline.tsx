import React from 'react';

/** Convert leading whitespace length to nesting depth (2-space indent). */
export function indentDepth(indent: string): number {
  return (indent.length / 2) | 0;
}

/**
 * Render inline markdown: **bold**, *italic*, `code`, [links](url), ![images](url),
 * bare HTTPS URLs, HTML tags.
 */
export function renderInline(text: string): React.ReactNode {
  // Strip remaining inline HTML comments
  const cleaned = text.replace(/<!--.*?-->/g, '');
  const parts: React.ReactNode[] = [];
  // Match: images, links, bold, italic, inline code, strikethrough, HTML tags, bare URLs
  const regex =
    /!\[([^\]]*)\]\(([^)]+)\)|\[([^\]]*)\]\(([^)]+)\)|\*\*(.+?)\*\*|\*(.+?)\*|`(.+?)`|~~(.+?)~~|<(sup|sub|strong|em|b|i)>(.*?)<\/\9>|<a\s+href="([^"]*)"[^>]*>(.*?)<\/a>|<img\s+[^>]*src="([^"]*)"[^>]*\/?>|(?<!["(])https:\/\/[^\s)<>\]]+/g;
  let last = 0;
  let key = 0;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(cleaned)) !== null) {
    if (match.index > last) {
      parts.push(cleaned.slice(last, match.index));
    }
    if (match[1] !== undefined) {
      // Image: ![alt](url)
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
      // Link: [text](url)
      const linkUrl = match[4] ?? '';
      const isSafe = linkUrl.startsWith('https://');
      parts.push(
        isSafe ? (
          <a
            key={key++}
            href={linkUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="text-muted-foreground hover:text-foreground hover:underline"
          >
            {match[3]}
          </a>
        ) : (
          <span key={key++} className="text-muted-foreground">
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
        <code
          key={key++}
          className="break-all rounded bg-secondary px-1 py-1 font-mono text-[11px]"
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
      // HTML <a> tag
      const aHref = match[11] ?? '';
      const aIsSafe = aHref.startsWith('https://');
      parts.push(
        aIsSafe ? (
          <a
            key={key++}
            href={aHref}
            target="_blank"
            rel="noopener noreferrer"
            className="text-muted-foreground hover:text-foreground hover:underline"
          >
            {match[12]}
          </a>
        ) : (
          <span key={key++} className="text-muted-foreground">
            {match[12]}
          </span>
        ),
      );
    } else if (match[13] !== undefined) {
      // HTML <img> tag
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
    } else if (match[0].startsWith('https://')) {
      // Bare HTTPS URL -- trim trailing punctuation that's likely sentence-ending
      let url = match[0];
      const trailingPunct = /[.,;:!?)]+$/;
      const trimmed = url.match(trailingPunct);
      if (trimmed) {
        url = url.slice(0, -trimmed[0].length);
      }
      parts.push(
        <a
          key={key++}
          href={url}
          target="_blank"
          rel="noopener noreferrer"
          className="text-muted-foreground hover:text-foreground hover:underline"
        >
          {url}
        </a>,
      );
      // Advance past the trimmed URL only; punctuation falls through as plain text
      last = match.index + url.length;
      continue;
    }
    last = match.index + match[0].length;
  }
  if (last < cleaned.length) {
    parts.push(cleaned.slice(last));
  }
  return parts.length === 1 ? parts[0] : <>{parts}</>;
}
