import React, { useRef, useState } from 'react';
import { ImageLightbox } from '#renderer/global/ui/ImageLightbox';
import { CodeBlock } from '#renderer/global/ui/primitives/code-block';
import { Separator } from '#renderer/global/ui/primitives/separator';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';
import {
  MarkdownTable,
  AdmonitionBlock,
  BlockquoteBlock,
  CheckboxItem,
  BulletItem,
  NumberedItem,
  HeadingBlock,
} from '#renderer/global/ui/PrMarkdownBlocks';
import { parseMarkdownBlocks, type MarkdownBlock } from '#renderer/global/service/markdownBlocks';

export function PrMarkdown({ text }: { text: string }): React.ReactElement {
  const [lightbox, setLightbox] = useState<{ images: string[]; index: number } | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const blocks = parseMarkdownBlocks(text);

  const handleClick = (e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.tagName !== 'IMG' || !target.getAttribute('data-lightbox-src')) return;
    e.stopPropagation();
    const el = containerRef.current;
    if (!el) return;
    const imgs = Array.from(el.querySelectorAll<HTMLImageElement>('img[data-lightbox-src]'));
    const urls = imgs.map((img) => img.src);
    const idx = imgs.indexOf(target as HTMLImageElement);
    if (idx !== -1 && urls.length > 0) setLightbox({ images: urls, index: idx });
  };

  return (
    <div ref={containerRef} onClick={handleClick}>
      {blocks.map((block, i) => (
        <BlockRenderer key={i} block={block} />
      ))}
      {lightbox && (
        <ImageLightbox
          images={lightbox.images}
          index={lightbox.index}
          onClose={() => setLightbox(null)}
          onNavigate={(i) => setLightbox((prev) => (prev ? { ...prev, index: i } : null))}
        />
      )}
    </div>
  );
}

function BlockRenderer({ block }: { block: MarkdownBlock }): React.ReactElement {
  switch (block.kind) {
    case 'code':
      return <CodeBlock code={block.code} language={block.language} />;
    case 'details':
      return (
        <details className="my-2 rounded border border-border px-3 py-2">
          <summary className="cursor-pointer text-[12px] font-medium text-foreground select-none">
            <InlineMarkdown text={block.summaryText} />
          </summary>
          <div className="mt-2 text-[12px]">
            <PrMarkdown text={block.body} />
          </div>
        </details>
      );
    case 'table':
      return <MarkdownTable headerCells={block.headerCells} rows={block.rows} />;
    case 'separator':
      return <Separator className="my-3" />;
    case 'heading':
      return <HeadingBlock level={block.level} text={block.text} />;
    case 'admonition':
      return <AdmonitionBlock type={block.type} bodyLines={block.bodyLines} />;
    case 'blockquote':
      return <BlockquoteBlock lines={block.lines} />;
    case 'checkbox':
      return <CheckboxItem checked={block.checked} depth={block.depth} text={block.text} />;
    case 'bullet':
      return <BulletItem depth={block.depth} text={block.text} />;
    case 'numbered':
      return <NumberedItem num={block.num} depth={block.depth} text={block.text} />;
    case 'empty':
      return <div className="h-2" />;
    case 'paragraph':
      return (
        <div className="break-words py-1 text-body text-foreground">
          <InlineMarkdown text={block.text} />
        </div>
      );
    case 'plain':
      return <div className="py-1 text-body text-foreground">{block.text}</div>;
  }
}
