import React, { useMemo, useState } from 'react';
import type { UserToolResultBlock } from '#renderer/global/types';
import { extractToolResultText } from '#renderer/domains/sessions/service/transcriptRenderHelpers';
import { sessionToolResultImageUrl } from '#renderer/domains/sessions/runtime/transcriptMedia';
import { ImageLightbox } from '#renderer/global/ui/ImageLightbox';
import { useTranscriptSessionId } from '#renderer/domains/sessions/ui/transcriptEvents/TranscriptSessionContext';

interface ToolResultBodyProps {
  result?: UserToolResultBlock;
  fallbackLang?: string;
  sessionId?: string;
  toolUseId?: string;
}

export function ToolResultBody({
  result,
  fallbackLang = 'text',
  sessionId,
  toolUseId,
}: ToolResultBodyProps): React.ReactNode {
  const transcriptSessionId = useTranscriptSessionId();
  const resolvedSessionId = sessionId ?? transcriptSessionId;
  const resolvedToolUseId = toolUseId ?? result?.toolUseId;
  const [lightboxIndex, setLightboxIndex] = useState<number | null>(null);
  const imageCount =
    result?.content.kind === 'blocks'
      ? result.content.data.blocks.filter((block) => block.kind === 'image').length
      : 0;
  const imageUrls = useMemo(() => {
    if (!resolvedSessionId || !resolvedToolUseId) return [];
    return Array.from({ length: imageCount }, (_, index) =>
      sessionToolResultImageUrl(resolvedSessionId, resolvedToolUseId, index),
    );
  }, [imageCount, resolvedSessionId, resolvedToolUseId]);
  if (!result) return null;
  const text = extractToolResultText(result);
  if (!text && imageUrls.length === 0) return null;
  const tone = result.isError ? 'text-destructive' : 'text-muted-foreground';
  return (
    <>
      {imageUrls.length > 0 && (
        <div className="mt-2 grid gap-2">
          {imageUrls.map((url, index) => (
            <button
              key={url}
              type="button"
              className="overflow-hidden rounded border border-border bg-muted/30 text-left transition-opacity hover:opacity-80"
              aria-label={`Open tool result image ${index + 1}`}
              onClick={() => setLightboxIndex(index)}
            >
              <img
                src={url}
                alt={`Tool result image ${index + 1}`}
                className="max-h-[36rem] w-full object-contain"
                data-testid="session-tool-result-image"
              />
            </button>
          ))}
        </div>
      )}
      {text && (
        <pre
          className={`mt-2 max-h-60 overflow-auto rounded bg-muted/60 p-2 text-label ${tone}`}
          data-language={fallbackLang}
        >
          {text}
        </pre>
      )}
      {lightboxIndex !== null && (
        <ImageLightbox
          images={imageUrls}
          index={lightboxIndex}
          onClose={() => setLightboxIndex(null)}
          onNavigate={setLightboxIndex}
        />
      )}
    </>
  );
}
