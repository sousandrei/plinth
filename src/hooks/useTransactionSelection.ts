import { useCallback, useMemo, useState } from 'react';

export interface TransactionSelection {
  selectedIds: string[];
  count: number;
  isSelected: (id: string) => boolean;
  toggle: (id: string) => void;
  toggleAll: (pageIds: string[], selectAll: boolean) => void;
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

  // selectAll is the *intended* new state passed down from the table header,
  // which already computed allSelected correctly.  Avoids re-deriving it here
  // from stale prev state, which could disagree with the visible checkbox.
  const toggleAll = useCallback((pageIds: string[], selectAll: boolean) => {
    if (selectAll) {
      setSelectedIds((prev) => {
        const merged = new Set(prev);
        for (const id of pageIds) merged.add(id);
        return Array.from(merged);
      });
    } else {
      setSelectedIds((prev) => prev.filter((id) => !pageIds.includes(id)));
    }
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
