import { subscribeObsDegraded } from '#renderer/global/providers/obsHealth';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { toast } from '#renderer/global/runtime/useFeedback';

export function useFeedbackObsDegraded(): void {
  useMountEffect(() =>
    subscribeObsDegraded(() => {
      toast.error('Observability pipeline degraded -- logs not being sent');
    }),
  );
}
