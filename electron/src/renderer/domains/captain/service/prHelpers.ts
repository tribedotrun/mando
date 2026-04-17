/** Strip review-bot badges, footers, and trailing HRs. */
export function stripBotContent(text: string): string {
  return (
    text
      // Devin review badge
      .replace(/<!-- devin-review-badge-begin -->[\s\S]*?<!-- devin-review-badge-end -->/g, '')
      // PR summary head marker
      .replace(/<!-- pr-summary-head:.*?-->/g, '')
      // Cursor agent badge (trailing <div> with cursor.com links after body end marker)
      .replace(/<!-- CURSOR_AGENT_PR_BODY_END -->[\s\S]*/g, '')
      .replace(/<!-- CURSOR_AGENT_PR_BODY_BEGIN -->\n?/g, '')
      // CodeRabbit auto-generated comments
      .replace(
        /<!-- This is an auto-generated comment: release notes by coderabbit\.ai -->[\s\S]*?<!-- end of auto-generated comment: release notes by coderabbit\.ai -->/g,
        '',
      )
      // Greptile comment blocks
      .replace(/<!-- greptile[\s\S]*?-->/g, '')
      // Trailing HRs
      .replace(/(\n\s*---\s*)+\s*$/g, '')
      .trim()
  );
}

/** Detect lines with Unicode box-drawing characters and wrap consecutive runs in fenced code blocks. */
const BOX_DRAWING_RE = /[┌┐└┘│─├┤┬┴┼▼▲◄►╔╗╚╝║═╠╣╦╩╬]/;

export function wrapAsciiArt(text: string): string {
  const lines = text.split('\n');
  const result: string[] = [];
  let i = 0;
  while (i < lines.length) {
    if (BOX_DRAWING_RE.test(lines[i])) {
      const group: string[] = [lines[i]];
      i++;
      while (i < lines.length) {
        if (BOX_DRAWING_RE.test(lines[i])) {
          group.push(lines[i]);
          i++;
        } else if (
          lines[i].trim() === '' &&
          i + 1 < lines.length &&
          BOX_DRAWING_RE.test(lines[i + 1])
        ) {
          group.push(lines[i]);
          i++;
        } else {
          break;
        }
      }
      result.push('```', ...group, '```');
    } else {
      result.push(lines[i]);
      i++;
    }
  }
  return result.join('\n');
}
