import type { ResultOf } from '#shared/ipc-contract';
import type { EvidenceDeckView } from '#renderer/domains/captain/types/evidenceDeck';

type EvidenceDeckSource = NonNullable<ResultOf<'shell:read-evidence-deck'>>;
type EvidenceDeckAsset = EvidenceDeckSource['assets'][number];

const HTML_ASSET_ATTRIBUTE = /\b(src|href|poster)=(['"])([^'"]+)\2/gi;
const CSS_ASSET_URL = /url\((['"]?)([^)'"\s]+)\1\)/gi;
const URL_SCHEME = /^[a-z][a-z\d+.-]*:/i;
const REVEAL_ROOT = /class=(['"])[^'"]*\breveal\b[^'"]*\1/i;
const REVEAL_VIEWPORT_STYLE =
  '<style data-mando-deck-viewer>body.reveal-viewport{background:var(--canvas,#000)!important}</style>';
const EMBED_HISTORY_GUARD = `<script data-mando-deck-runtime>
for (const method of ['pushState', 'replaceState']) {
  const original = history[method].bind(history);
  history[method] = (state, unused, url) => {
    try {
      return original(state, unused, url);
    } catch (error) {
      if (error instanceof DOMException && error.name === 'SecurityError') return undefined;
      throw error;
    }
  };
}
</script>`;

function injectBeforeHeadClose(html: string, fragment: string): string {
  return /<\/head>/i.test(html)
    ? html.replace(/<\/head>/i, `${fragment}</head>`)
    : `${fragment}${html}`;
}

function assetReferencePath(reference: string): string | null {
  if (reference.startsWith('#') || reference.startsWith('/') || URL_SCHEME.test(reference)) {
    return null;
  }
  const suffixIndex = reference.search(/[?#]/);
  const pathOnly = suffixIndex === -1 ? reference : reference.slice(0, suffixIndex);
  const withoutDotPrefix = pathOnly.replace(/^\.\//, '');
  try {
    return decodeURIComponent(withoutDotPrefix);
  } catch {
    return withoutDotPrefix;
  }
}

function assetDataUrl(asset: EvidenceDeckAsset): string {
  return `data:${asset.mime};base64,${asset.base64}`;
}

function replaceReference(reference: string, assets: ReadonlyMap<string, string>): string {
  const assetPath = assetReferencePath(reference);
  if (!assetPath) return reference;
  return assets.get(assetPath) ?? reference;
}

export function prepareEvidenceDeck(source: EvidenceDeckSource): EvidenceDeckView {
  const assets = new Map(source.assets.map((asset) => [asset.path, assetDataUrl(asset)]));
  const guardedHtml = injectBeforeHeadClose(source.html, EMBED_HISTORY_GUARD);
  const normalizedHtml = REVEAL_ROOT.test(source.html)
    ? injectBeforeHeadClose(guardedHtml, REVEAL_VIEWPORT_STYLE)
    : guardedHtml;
  const withHtmlAssets = normalizedHtml.replace(
    HTML_ASSET_ATTRIBUTE,
    (match, attribute: string, quote: string, reference: string) => {
      const replacement = replaceReference(reference, assets);
      return replacement === reference ? match : `${attribute}=${quote}${replacement}${quote}`;
    },
  );
  const html = withHtmlAssets.replace(CSS_ASSET_URL, (match, quote: string, reference: string) => {
    const replacement = replaceReference(reference, assets);
    return replacement === reference ? match : `url(${quote}${replacement}${quote})`;
  });
  return { html, modifiedAtMs: source.modifiedAtMs };
}
