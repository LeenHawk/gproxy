import { useCallback, useEffect, useMemo, useState } from "react";

export type BatchSelectionInput<Row, Key extends string | number> = {
  rows: readonly Row[];
  getKey: (row: Row, index: number) => Key;
  onBatchDelete: (keys: readonly Key[]) => Promise<void>;
  onSuccess?: (count: number) => void;
  onError?: (err: unknown) => void;
  /// Returns the text for the `window.confirm` prompt shown before delete.
  /// Callers should pass a localized message via the i18n `t` function.
  confirmMessage?: (count: number) => string;
};

export type BatchSelectionState<Key extends string | number> = {
  batchMode: boolean;
  enterBatch: () => void;
  exitBatch: () => void;
  selectedKeys: ReadonlySet<Key>;
  selectedCount: number;
  isSelected: (key: Key) => boolean;
  toggle: (key: Key) => void;
  selectAll: () => void;
  clear: () => void;
  deleteSelected: () => Promise<void>;
  pending: boolean;
};

/// Generic selection + batch-delete orchestration hook shared by every admin
/// table that supports batch deletion.
///
/// Key behaviors:
/// - `batchMode` is independent of `selectedKeys`. Entering batch mode does
///   not pre-select anything; exiting batch mode clears the selection.
/// - When `rows` changes and the new rows no longer contain a selected key,
///   that key is dropped. This keeps the selection consistent with whatever
///   the user can actually see after a concurrent refetch.
/// - `deleteSelected` confirms via `window.confirm` (matching the codebase's
///   existing delete UX elsewhere), then invokes `onBatchDelete`. On success
///   it clears the selection and exits batch mode; on failure it keeps both
///   and invokes `onError` so the call site can show a toast.
export function useBatchSelection<Row, Key extends string | number>(
  input: BatchSelectionInput<Row, Key>,
): BatchSelectionState<Key> {
  const { rows, getKey, onBatchDelete, onSuccess, onError, confirmMessage } = input;

  const [batchMode, setBatchMode] = useState(false);
  const [selectedKeys, setSelectedKeys] = useState<ReadonlySet<Key>>(() => new Set<Key>());
  const [pending, setPending] = useState(false);

  const visibleKeys = useMemo(() => {
    const set = new Set<Key>();
    rows.forEach((row, index) => set.add(getKey(row, index)));
    return set;
  }, [rows, getKey]);

  // Drop any selection whose key no longer exists in `rows`.
  useEffect(() => {
    setSelectedKeys((prev) => {
      let changed = false;
      const next = new Set<Key>();
      for (const key of prev) {
        if (visibleKeys.has(key)) {
          next.add(key);
        } else {
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [visibleKeys]);

  const enterBatch = useCallback(() => {
    setBatchMode(true);
  }, []);

  const exitBatch = useCallback(() => {
    setBatchMode(false);
    setSelectedKeys(new Set<Key>());
  }, []);

  const isSelected = useCallback(
    (key: Key) => selectedKeys.has(key),
    [selectedKeys],
  );

  const toggle = useCallback((key: Key) => {
    setSelectedKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  }, []);

  const selectAll = useCallback(() => {
    setSelectedKeys(new Set(visibleKeys));
  }, [visibleKeys]);

  const clear = useCallback(() => {
    setSelectedKeys(new Set<Key>());
  }, []);

  const deleteSelected = useCallback(async () => {
    if (selectedKeys.size === 0 || pending) return;
    const message = confirmMessage
      ? confirmMessage(selectedKeys.size)
      : `Delete ${selectedKeys.size} selected item(s)? This cannot be undone.`;
    if (!window.confirm(message)) return;
    const keys = Array.from(selectedKeys);
    setPending(true);
    try {
      await onBatchDelete(keys);
      setSelectedKeys(new Set<Key>());
      setBatchMode(false);
      onSuccess?.(keys.length);
    } catch (err) {
      onError?.(err);
    } finally {
      setPending(false);
    }
  }, [selectedKeys, pending, confirmMessage, onBatchDelete, onSuccess, onError]);

  return {
    batchMode,
    enterBatch,
    exitBatch,
    selectedKeys,
    selectedCount: selectedKeys.size,
    isSelected,
    toggle,
    selectAll,
    clear,
    deleteSelected,
    pending,
  };
}
