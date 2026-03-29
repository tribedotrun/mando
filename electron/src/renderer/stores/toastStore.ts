import { create } from 'zustand';

type ToastVariant = 'success' | 'error' | 'info';

interface ToastOptions {
  detail?: string;
  onUndo?: () => void;
}

export interface Toast {
  id: string;
  variant: ToastVariant;
  message: string;
  detail?: string;
  onUndo?: () => void;
  createdAt: number;
}

interface ToastStore {
  toasts: Toast[];
  add: (variant: ToastVariant, message: string, options?: ToastOptions) => void;
  dismiss: (id: string) => void;
}

let nextId = 0;

export const useToastStore = create<ToastStore>((set) => ({
  toasts: [],

  add: (variant, message, options) => {
    const id = String(++nextId);
    const toast: Toast = {
      id,
      variant,
      message,
      detail: options?.detail,
      onUndo: options?.onUndo,
      createdAt: Date.now(),
    };
    set((state) => {
      const updated = [...state.toasts, toast];
      return { toasts: updated.slice(-3) };
    });
  },

  dismiss: (id) => {
    set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) }));
  },
}));
