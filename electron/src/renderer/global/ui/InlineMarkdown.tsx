import React from 'react';
import {
  HTML_TAG_MAP,
  parseInlineMarkdown,
  type InlineToken,
} from '#renderer/global/service/inlineMarkdownHelpers';

function TokenNode({ token, index: key }: { token: InlineToken; index: number }): React.ReactNode {
  switch (token.type) {
    case 'text':
      return token.value;
    case 'image':
      return (
        <img
          key={key}
          src={token.src}
          alt={token.alt}
          data-lightbox-src={token.src}
          className="my-1 max-h-[300px] max-w-full cursor-pointer rounded transition-opacity hover:opacity-80"
        />
      );
    case 'image-placeholder':
      return (
        <span key={key} className="italic text-text-3">
          [{token.alt}]
        </span>
      );
    case 'link':
      return token.safe ? (
        <a
          key={key}
          href={token.href}
          target="_blank"
          rel="noopener noreferrer"
          className="text-muted-foreground hover:text-foreground hover:underline"
        >
          {token.text}
        </a>
      ) : (
        <span key={key} className="text-muted-foreground">
          {token.text}
        </span>
      );
    case 'bold':
      return (
        <strong key={key} className="font-semibold text-foreground">
          {token.text}
        </strong>
      );
    case 'italic':
      return <em key={key}>{token.text}</em>;
    case 'code':
      return (
        <code key={key} className="break-all rounded bg-secondary px-1 py-1 font-mono text-[11px]">
          {token.text}
        </code>
      );
    case 'strike':
      return (
        <del key={key} className="text-text-3">
          {token.text}
        </del>
      );
    case 'html-inline': {
      const Tag = (HTML_TAG_MAP[token.tag] ?? 'span') as keyof React.JSX.IntrinsicElements;
      return <Tag key={key}>{token.content}</Tag>;
    }
    case 'html-a':
      return token.safe ? (
        <a
          key={key}
          href={token.href}
          target="_blank"
          rel="noopener noreferrer"
          className="text-muted-foreground hover:text-foreground hover:underline"
        >
          {token.text}
        </a>
      ) : (
        <span key={key} className="text-muted-foreground">
          {token.text}
        </span>
      );
    case 'html-img':
      return token.safe ? (
        <img
          key={key}
          src={token.src}
          alt=""
          data-lightbox-src={token.src}
          className="my-1 max-h-[300px] max-w-full cursor-pointer rounded transition-opacity hover:opacity-80"
        />
      ) : (
        <span key={key} className="italic text-text-3">
          [image]
        </span>
      );
    case 'bare-url':
      return (
        <a
          key={key}
          href={token.url}
          target="_blank"
          rel="noopener noreferrer"
          className="text-muted-foreground hover:text-foreground hover:underline"
        >
          {token.url}
        </a>
      );
    default:
      return null;
  }
}

/**
 * Render inline markdown: **bold**, *italic*, `code`, [links](url), ![images](url),
 * bare HTTPS URLs, HTML tags.
 */
export function InlineMarkdown({ text }: { text: string }): React.ReactNode {
  const tokens = parseInlineMarkdown(text);
  const parts = tokens.map((t, i) => <TokenNode key={i} token={t} index={i} />);
  return parts.length === 1 ? parts[0] : <>{parts}</>;
}
