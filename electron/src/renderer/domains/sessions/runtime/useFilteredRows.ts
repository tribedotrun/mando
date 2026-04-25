import { Children, isValidElement, useMemo, type ReactNode } from 'react';

/**
 * Extract visible text content from a React node tree by walking children
 * recursively. Ignores element types, prop names, and className strings —
 * matching those by accident is the `JSON.stringify(row)` trap.
 */
function extractText(node: ReactNode): string {
  if (node == null || typeof node === 'boolean') return '';
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(extractText).join(' ');
  if (isValidElement(node)) {
    const children = (node.props as { children?: ReactNode })?.children;
    return Children.toArray(children).map(extractText).join(' ');
  }
  return '';
}

export function useFilteredTranscriptRows(rows: ReactNode[], query: string): ReactNode[] {
  return useMemo(() => {
    const trimmed = query.trim().toLowerCase();
    if (!trimmed) return rows;
    return rows.filter((row) => {
      if (row == null) return false;
      return extractText(row).toLowerCase().includes(trimmed);
    });
  }, [rows, query]);
}
