/** Extracts the first image file from a clipboard paste event, or null if none found. */
export function extractImageFromClipboard(e: React.ClipboardEvent): File | null {
  for (const item of e.clipboardData.items) {
    if (!item.type.startsWith('image/')) continue;
    e.preventDefault();
    return item.getAsFile();
  }
  return null;
}
