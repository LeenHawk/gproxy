import { useCallback, useMemo, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { batchOp, type BatchOp, type BatchOutcome } from "@/api/batch";

type Id = number | string;

interface UseBatchOptions {
  onSuccess?: (operation: BatchOp, ids: Id[], outcome: BatchOutcome) => void;
}

/** 选择模式状态 + 批量 mutation。`invalidateKey` 为成功后失效的 query key。 */
export function useBatch(entity: string, invalidateKey: unknown[], options?: UseBatchOptions) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [mode, setModeRaw] = useState(false);
  const [selected, setSelected] = useState<Set<Id>>(new Set());

  const setMode = useCallback((on: boolean) => {
    setModeRaw(on);
    if (!on) setSelected(new Set());
  }, []);

  const toggle = useCallback((id: Id) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const exit = useCallback(() => {
    setMode(false);
    setSelected(new Set());
  }, [setMode]);

  const mutation = useMutation({
    mutationFn: async (operation: BatchOp) => {
      const ids = [...selected];
      return { operation, ids, outcome: await batchOp(entity, operation, ids) };
    },
    onSuccess: ({ operation, ids, outcome }) => {
      void qc.invalidateQueries({ queryKey: invalidateKey });
      if (outcome.errors.length > 0) {
        toast.warning(t("batch.partial", { ok: outcome.affected, fail: outcome.errors.length }));
      } else {
        toast.success(t("batch.done", { count: outcome.affected }));
      }
      options?.onSuccess?.(operation, ids, outcome);
      exit();
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : String(e)),
  });

  // 当前页全选/全不选辅助:页面把当前行 id 传进来。
  const helpers = useMemo(
    () => ({
      toggleAllFor: (ids: Id[]) =>
        setSelected((prev) => (ids.length > 0 && ids.every((id) => prev.has(id)) ? new Set<Id>() : new Set(ids))),
      allSelectedFor: (ids: Id[]) => ids.length > 0 && ids.every((id) => selected.has(id)),
    }),
    [selected],
  );

  return {
    mode,
    setMode,
    selected,
    toggle,
    exit,
    pending: mutation.isPending,
    runEnable: () => mutation.mutate("enable"),
    runDisable: () => mutation.mutate("disable"),
    runDelete: () => mutation.mutate("delete"),
    ...helpers,
  };
}
