import React, { useRef, useState } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import rehypeRaw from 'rehype-raw';
import rehypeSanitize, { defaultSchema, type Options } from 'rehype-sanitize';
import remarkGfm from 'remark-gfm';
import { resolveGitHubUserAttachmentUrl } from '#renderer/domains/captain/runtime/githubUserAttachments';
import { isBareGitHubUserAttachment } from '#renderer/global/service/githubUserAttachments';
import { ImageLightbox } from '#renderer/global/ui/ImageLightbox';

const githubMarkdownSchema: Options = {
  ...defaultSchema,
  tagNames: [...(defaultSchema.tagNames ?? []), 'video'],
  attributes: {
    ...defaultSchema.attributes,
    video: [
      'controls',
      'height',
      'loop',
      'muted',
      'playsInline',
      'poster',
      'preload',
      'src',
      'width',
    ],
    source: ['src', 'type'],
  },
};

function nodeText(children: React.ReactNode): string {
  return React.Children.toArray(children)
    .filter((child): child is string => typeof child === 'string')
    .join('');
}

const components: Components = {
  h1: ({ children }) => <h1 className="mt-6 mb-3 text-heading text-foreground">{children}</h1>,
  h2: ({ children }) => (
    <h2 className="mt-6 mb-2 border-b border-border pb-2 text-subheading text-foreground">
      {children}
    </h2>
  ),
  h3: ({ children }) => <h3 className="mt-5 mb-2 text-body font-semibold">{children}</h3>,
  h4: ({ children }) => <h4 className="mt-4 mb-1 text-caption font-semibold">{children}</h4>,
  p: ({ children }) => <p className="my-3 text-body leading-6 text-foreground">{children}</p>,
  a: ({ href, children }) => {
    const label = nodeText(children);
    if (isBareGitHubUserAttachment(href, label)) {
      return (
        <video
          controls
          preload="metadata"
          src={resolveGitHubUserAttachmentUrl(href)}
          className="my-3 max-h-[560px] max-w-full rounded-md border border-border bg-black"
        >
          <a href={href}>Open recording on GitHub</a>
        </video>
      );
    }
    return (
      <a
        href={href}
        target="_blank"
        rel="noopener noreferrer"
        className="break-all text-muted-foreground hover:text-foreground hover:underline"
      >
        {children}
      </a>
    );
  },
  img: ({ src, alt }) => {
    const resolved = resolveGitHubUserAttachmentUrl(typeof src === 'string' ? src : undefined);
    return (
      <img
        src={resolved}
        alt={alt ?? ''}
        data-lightbox-src={resolved}
        className="my-2 max-h-[560px] max-w-full cursor-pointer rounded-md border border-border object-contain transition-opacity hover:opacity-80"
      />
    );
  },
  video: ({ src, poster, children }) => (
    <video
      controls
      preload="metadata"
      src={resolveGitHubUserAttachmentUrl(typeof src === 'string' ? src : undefined)}
      poster={resolveGitHubUserAttachmentUrl(typeof poster === 'string' ? poster : undefined)}
      className="my-3 max-h-[560px] max-w-full rounded-md border border-border bg-black"
    >
      {children}
    </video>
  ),
  source: ({ src, type }) => (
    <source
      src={resolveGitHubUserAttachmentUrl(typeof src === 'string' ? src : undefined)}
      type={type}
    />
  ),
  details: ({ children }) => (
    <details className="my-3 rounded-md border border-border px-3 py-2">{children}</details>
  ),
  summary: ({ children }) => (
    <summary className="cursor-pointer text-body font-medium text-foreground select-none">
      {children}
    </summary>
  ),
  blockquote: ({ children }) => (
    <blockquote className="my-3 border-l-4 border-muted-foreground/30 pl-4 text-text-3">
      {children}
    </blockquote>
  ),
  ul: ({ className, children }) => (
    <ul
      className={`my-3 space-y-1 pl-6 text-body ${className?.includes('contains-task-list') ? 'list-none pl-1' : 'list-disc'}`}
    >
      {children}
    </ul>
  ),
  ol: ({ className, children, start }) => (
    <ol
      start={start}
      className={`my-3 space-y-1 pl-7 text-body ${className?.includes('contains-task-list') ? 'list-none pl-1' : 'list-decimal'}`}
    >
      {children}
    </ol>
  ),
  li: ({ children }) => <li className="pl-1 leading-6">{children}</li>,
  input: ({ type, checked }) =>
    type === 'checkbox' ? (
      <input
        type="checkbox"
        checked={checked}
        disabled
        readOnly
        className="mr-2 h-3.5 w-3.5 align-middle accent-foreground"
      />
    ) : null,
  strong: ({ children }) => <strong className="font-semibold text-foreground">{children}</strong>,
  del: ({ children }) => <del className="text-text-3">{children}</del>,
  hr: () => <hr className="my-5 border-border" />,
  table: ({ children }) => (
    <div className="my-4 max-w-full overflow-x-auto rounded-md border border-border">
      <table className="w-full border-collapse text-caption">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border-b border-r border-border bg-secondary px-3 py-2 text-left font-medium last:border-r-0">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border-r border-t border-border px-3 py-2 align-top last:border-r-0">
      {children}
    </td>
  ),
  pre: ({ children }) => (
    <pre className="my-3 max-w-full overflow-x-auto rounded-md border border-border bg-secondary p-3 font-mono text-[11px] leading-5">
      {children}
    </pre>
  ),
  code: ({ className, children }) =>
    className ? (
      <code className={className}>{children}</code>
    ) : (
      <code className="break-words rounded bg-secondary px-1 py-0.5 font-mono text-[11px]">
        {children}
      </code>
    ),
};

export function GitHubPrMarkdown({ text }: { text: string }): React.ReactElement {
  const [lightbox, setLightbox] = useState<{ images: string[]; index: number } | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const handleClick = (event: React.MouseEvent<HTMLDivElement>) => {
    const target = event.target;
    if (!(target instanceof HTMLImageElement) || !target.dataset.lightboxSrc) return;
    event.stopPropagation();
    const images = Array.from(
      containerRef.current?.querySelectorAll<HTMLImageElement>('img[data-lightbox-src]') ?? [],
    );
    const index = images.indexOf(target);
    if (index >= 0) setLightbox({ images: images.map((image) => image.src), index });
  };

  return (
    <div
      ref={containerRef}
      onClick={handleClick}
      className="min-w-0 max-w-full [overflow-wrap:anywhere]"
    >
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeRaw, [rehypeSanitize, githubMarkdownSchema]]}
        components={components}
      >
        {text}
      </ReactMarkdown>
      {lightbox && (
        <ImageLightbox
          images={lightbox.images}
          index={lightbox.index}
          onClose={() => setLightbox(null)}
          onNavigate={(index) =>
            setLightbox((previous) => (previous ? { ...previous, index } : null))
          }
        />
      )}
    </div>
  );
}
