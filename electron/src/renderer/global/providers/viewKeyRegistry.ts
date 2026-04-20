// Shared registry for view-level keyboard handlers. Multiple views stay mounted
// (hidden via display:none to avoid flicker); each registers a handler + active
// ref. Dispatch invokes the single entry whose activeRef is currently true.

type ViewKeyHandler = (key: string, e: KeyboardEvent) => void;

export interface ViewEntry {
  handler: ViewKeyHandler;
  activeRef: React.RefObject<boolean>;
}

const viewHandlers = new Set<ViewEntry>();

export function registerViewHandler(entry: ViewEntry): () => void {
  viewHandlers.add(entry);
  return () => {
    viewHandlers.delete(entry);
  };
}

export function dispatchToActiveView(key: string, e: KeyboardEvent): void {
  for (const entry of viewHandlers) {
    if (entry.activeRef.current) {
      entry.handler(key, e);
      return;
    }
  }
}
