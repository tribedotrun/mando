/**
 * Inline markdown token types and parser.
 * Render inline markdown: **bold**, *italic*, `code`, [links](url), ![images](url),
 * bare HTTPS URLs, HTML tags.
 */

export const HTML_TAG_MAP: Record<string, string> = {
  sup: 'sup',
  sub: 'sub',
  strong: 'strong',
  em: 'em',
  b: 'strong',
  i: 'em',
};

export type InlineToken =
  | { type: 'text'; value: string }
  | { type: 'image'; src: string; alt: string }
  | { type: 'image-placeholder'; alt: string }
  | { type: 'link'; href: string; text: string; safe: boolean }
  | { type: 'bold'; text: string }
  | { type: 'italic'; text: string }
  | { type: 'code'; text: string }
  | { type: 'strike'; text: string }
  | { type: 'html-inline'; tag: string; content: string }
  | { type: 'html-a'; href: string; text: string; safe: boolean }
  | { type: 'html-img'; src: string; safe: boolean }
  | { type: 'bare-url'; url: string };

// Match: images, links, bold, italic, inline code, strikethrough, HTML tags, bare URLs
const INLINE_REGEX =
  /!\[([^\]]*)\]\(([^)]+)\)|\[([^\]]*)\]\(([^)]+)\)|\*\*(.+?)\*\*|\*(.+?)\*|`(.+?)`|~~(.+?)~~|<(sup|sub|strong|em|b|i)>(.*?)<\/\9>|<a\s+href="([^"]*)"[^>]*>(.*?)<\/a>|<img\s+[^>]*src="([^"]*)"[^>]*\/?>|(?<!["(])https:\/\/[^\s)<>\]]+/g;

const TRAILING_PUNCT = /[.,;:!?)]+$/;

export function parseInlineMarkdown(text: string): InlineToken[] {
  const cleaned = text.replace(/<!--.*?-->/g, '');
  const tokens: InlineToken[] = [];
  let last = 0;
  let match: RegExpExecArray | null;
  const regex = new RegExp(INLINE_REGEX.source, INLINE_REGEX.flags);

  while ((match = regex.exec(cleaned)) !== null) {
    if (match.index > last) {
      tokens.push({ type: 'text', value: cleaned.slice(last, match.index) });
    }

    if (match[1] !== undefined) {
      // Image: ![alt](url)
      const src = match[2] ?? '';
      tokens.push(
        src.startsWith('https://')
          ? { type: 'image', src, alt: match[1] }
          : { type: 'image-placeholder', alt: match[1] || 'image' },
      );
    } else if (match[3] !== undefined) {
      // Link: [text](url)
      const href = match[4] ?? '';
      tokens.push({ type: 'link', href, text: match[3], safe: href.startsWith('https://') });
    } else if (match[5]) {
      tokens.push({ type: 'bold', text: match[5] });
    } else if (match[6]) {
      tokens.push({ type: 'italic', text: match[6] });
    } else if (match[7]) {
      tokens.push({ type: 'code', text: match[7] });
    } else if (match[8]) {
      tokens.push({ type: 'strike', text: match[8] });
    } else if (match[9]) {
      tokens.push({ type: 'html-inline', tag: match[9], content: match[10] ?? '' });
    } else if (match[11] !== undefined) {
      const href = match[11] ?? '';
      tokens.push({
        type: 'html-a',
        href,
        text: match[12] ?? '',
        safe: href.startsWith('https://'),
      });
    } else if (match[13] !== undefined) {
      const src = match[13] ?? '';
      tokens.push({ type: 'html-img', src, safe: src.startsWith('https://') });
    } else if (match[0].startsWith('https://')) {
      // Bare HTTPS URL -- trim trailing punctuation that's likely sentence-ending
      let url = match[0];
      const trimmed = url.match(TRAILING_PUNCT);
      if (trimmed) url = url.slice(0, -trimmed[0].length);
      tokens.push({ type: 'bare-url', url });
      last = match.index + url.length;
      continue;
    }

    last = match.index + match[0].length;
  }

  if (last < cleaned.length) {
    tokens.push({ type: 'text', value: cleaned.slice(last) });
  }

  return tokens;
}
