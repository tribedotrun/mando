import { indentDepth } from '#renderer/global/service/markdownHelpers';

export type MarkdownBlock =
  | { kind: 'code'; language: string; code: string }
  | { kind: 'details'; summaryText: string; body: string }
  | { kind: 'table'; headerCells: string[]; rows: string[][] }
  | { kind: 'separator' }
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'admonition'; type: string; bodyLines: string[] }
  | { kind: 'blockquote'; lines: string[] }
  | { kind: 'checkbox'; checked: boolean; depth: number; text: string }
  | { kind: 'bullet'; depth: number; text: string }
  | { kind: 'numbered'; num: string; depth: number; text: string }
  | { kind: 'empty' }
  | { kind: 'paragraph'; text: string }
  | { kind: 'plain'; text: string };

/** Parse markdown text into a flat list of block descriptors. */
export function parseMarkdownBlocks(text: string): MarkdownBlock[] {
  const cleaned = text.replace(/<!--[\s\S]*?-->/g, '').replace(/\n{3,}/g, '\n\n');
  const lines = cleaned.split('\n');
  const blocks: MarkdownBlock[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (line.trimStart().startsWith('```')) {
      const language = line.trimStart().slice(3).trim();
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
        blocks.push({ kind: 'plain', text: line });
        continue;
      }
      blocks.push({ kind: 'code', language: language || 'text', code: codeLines.join('\n') });
      continue;
    }

    if (line.trimStart().startsWith('<details')) {
      const detailLines: string[] = [line];
      i++;
      let depth = 1;
      if (line.trimStart().includes('</details>')) depth--;
      while (i < lines.length && depth > 0) {
        const cur = lines[i];
        if (cur.trimStart().startsWith('<details')) depth++;
        if (cur.trimStart().includes('</details>') && depth > 0) depth--;
        detailLines.push(cur);
        i++;
      }
      const joined = detailLines.join('\n');
      const summaryMatch = joined.match(/<summary>([\s\S]*?)<\/summary>/);
      const summaryText = summaryMatch ? summaryMatch[1].trim() : 'Details';
      const body = joined
        .replace(/<details[^>]*>/, '')
        .replace(/<summary>[\s\S]*?<\/summary>/, '')
        .replace(/<\/details>\s*$/, '')
        .trim();
      blocks.push({ kind: 'details', summaryText, body });
      continue;
    }

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
      blocks.push({ kind: 'table', headerCells, rows });
      continue;
    }

    if (/^---+$|^\*\*\*+$|^___+$/.test(line.trim())) {
      blocks.push({ kind: 'separator' });
      i++;
      continue;
    }

    const headingMatch = line.match(/^(#{1,4})\s+(.*)/);
    if (headingMatch) {
      blocks.push({ kind: 'heading', level: headingMatch[1].length, text: headingMatch[2] });
      i++;
      continue;
    }

    if (line.startsWith('> ')) {
      const content = line.slice(2);
      const admonition = content.match(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]/);
      if (admonition) {
        const type = admonition[1];
        const bodyLines: string[] = [];
        const afterTag = content.slice(admonition[0].length).trim();
        if (afterTag) bodyLines.push(afterTag);
        i++;
        while (i < lines.length && lines[i].startsWith('> ')) {
          bodyLines.push(lines[i].slice(2));
          i++;
        }
        blocks.push({ kind: 'admonition', type, bodyLines });
        continue;
      }
      const bqLines = [content];
      i++;
      while (i < lines.length && lines[i].startsWith('> ')) {
        bqLines.push(lines[i].slice(2));
        i++;
      }
      blocks.push({ kind: 'blockquote', lines: bqLines });
      continue;
    }

    const checkMatch = line.match(/^(\s*)[-*]\s+\[([ xX])\]\s+(.*)/);
    if (checkMatch) {
      blocks.push({
        kind: 'checkbox',
        checked: checkMatch[2] !== ' ',
        depth: indentDepth(checkMatch[1]),
        text: checkMatch[3],
      });
      i++;
      continue;
    }

    const ulMatch = line.match(/^(\s*)[-*]\s+(.*)/);
    if (ulMatch) {
      blocks.push({ kind: 'bullet', depth: indentDepth(ulMatch[1]), text: ulMatch[2] });
      i++;
      continue;
    }

    const olMatch = line.match(/^(\s*)\d+\.\s+(.*)/);
    if (olMatch) {
      const num = line.match(/^(\s*)(\d+)\./)?.[2] ?? '1';
      blocks.push({ kind: 'numbered', num, depth: indentDepth(olMatch[1]), text: olMatch[2] });
      i++;
      continue;
    }

    if (line.trim() === '') {
      blocks.push({ kind: 'empty' });
      i++;
      continue;
    }

    blocks.push({ kind: 'paragraph', text: line });
    i++;
  }

  return blocks;
}
