import type { ImageViewInput } from '#renderer/global/types';
import { compactDisplayPath } from '#renderer/domains/sessions/service/transcriptRenderHelpers';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';

export function ImageViewBlock({ id, input }: { id: string; input: ImageViewInput }) {
  return <ToolFrame id={id} name="Viewed" summary={compactDisplayPath(input.path)} />;
}
