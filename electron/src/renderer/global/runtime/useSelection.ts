import { useState, useCallback } from 'react';

interface HasId {
  id: number;
}

interface UseSelectionResult {
  selectedIds: Set<number>;
  toggleSelect: (id: number) => void;
  toggleSelectAll: (visible: HasId[]) => void;
  clearSelection: () => void;
}

export function useSelection(): UseSelectionResult {
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());

  const toggleSelect = useCallback((id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const toggleSelectAll = useCallback((visible: HasId[]) => {
    setSelectedIds((prev) => {
      const allSelected = visible.length > 0 && visible.every((b) => prev.has(b.id));
      if (allSelected) return new Set<number>();
      return new Set(visible.map((b) => b.id));
    });
  }, []);

  const clearSelection = useCallback(() => setSelectedIds(new Set()), []);

  return { selectedIds, toggleSelect, toggleSelectAll, clearSelection };
}
