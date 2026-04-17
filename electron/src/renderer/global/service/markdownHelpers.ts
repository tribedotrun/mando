/** Convert leading whitespace length to nesting depth (2-space indent). */
export function indentDepth(indent: string): number {
  return (indent.length / 2) | 0;
}
