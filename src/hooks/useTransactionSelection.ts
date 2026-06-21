import { useCallback, useMemo, useState } from 'react';

export interface TransactionSelection {
  selectedIds: string[];
  count: number;
  isSelected: (id: string) => boolean;
  toggle: (id: string) => void;
  toggleAll: (pageIds: string[]) => void;
  clear: () => void;
}

export const useTransactionSelection = (): TransactionSelection => {
  const [selectedIds, setSelectedIds] = useState<string[]>([]);

  const isSelected = useCallback(
    (id: string) => selectedIds.includes(id),
    [selectedIds],
  );

  const toggle = useCallback((id: string) => {
    setSelectedIds((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id],
    );
  }, []);

  const toggleAll = useCallback((pageIds: string[]) => {
    setSelectedIds((prev) => {
      const allSelected = pageIds.every((id) => prev.includes(id));
      if (allSelected) {
        return prev.filter((id) => !pageIds.includes(id));
      }
      const merged = new Set(prev);
      for (const id of pageIds) merged.add(id);
      return Array.from(merged);
    });
  }, []);

  const clear = useCallback(() => setSelectedIds([]), []);

  return useMemo(
    () => ({
      selectedIds,
      count: selectedIds.length,
      isSelected,
      toggle,
      toggleAll,
      clear,
    }),
    [selectedIds, isSelected, toggle, toggleAll, clear],
  );
};
