import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  upsertRoutingRule, OPERATIONS, KINDS, IMPLEMENTATIONS, type RoutingRule,
} from "@/api/rules";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";

/** Destination protocols for transform_to are the four content-generation kinds. */
const DEST_KINDS = KINDS.slice(0, 4);

export interface CellInitial {
  operation: string;
  kind: string;
  implementation: string;
  destKind: string | null;
  ruleId?: number;
  sortOrder?: number;
}

/** Edits the routing behavior of one (operation, kind) cell. In "add" mode the
 *  operation/kind are selectable; in "edit" mode they are fixed (you're changing
 *  that cell). Saving upserts the underlying routing rule; the table refetches so
 *  the new effective behavior shows immediately. */
export function RoutingCellEditor({
  providerId, mode, initial, onSaved,
}: {
  providerId: number;
  mode: "add" | "edit";
  initial: CellInitial;
  onSaved: () => void;
}) {
  const { t } = useTranslation("rules");
  const queryClient = useQueryClient();

  const [operation, setOperation] = useState(initial.operation);
  const [kind, setKind] = useState(initial.kind);
  const [implementation, setImplementation] = useState(initial.implementation);
  const [destKind, setDestKind] = useState(initial.destKind ?? DEST_KINDS[2]); // claude_messages
  const [formError, setFormError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: (): Promise<RoutingRule> =>
      upsertRoutingRule(providerId, {
        id: initial.ruleId ?? null,
        provider_id: providerId,
        operation,
        kind,
        implementation,
        dest_operation: null, // backend defaults to the source operation
        dest_kind: implementation === "transform_to" ? destKind : null,
        sort_order: initial.sortOrder ?? 0,
        enabled: true,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["providers", providerId, "routing-rules"] });
      onSaved();
    },
    onError: (e) => setFormError(e instanceof ApiError ? e.message : String(e)),
  });

  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => { e.preventDefault(); setFormError(null); mutation.mutate(); }}
    >
      {mode === "add" ? (
        <>
          <div className="grid gap-2">
            <Label>{t("routing.columns.operation")}</Label>
            <Select value={operation} onValueChange={setOperation}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                {OPERATIONS.map((op) => (
                  <SelectItem key={op} value={op}>{t(`operation.${op}`)}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="grid gap-2">
            <Label>{t("routing.columns.kind")}</Label>
            <Select value={kind} onValueChange={setKind}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                {KINDS.map((k) => (
                  <SelectItem key={k} value={k}>{t(`protocolKind.${k}`)}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </>
      ) : (
        <div className="rounded-md border bg-muted/30 px-3 py-2 text-sm">
          <span className="font-medium">{t(`operation.${operation}`)}</span>
          <span className="text-muted-foreground"> · {t(`protocolKind.${kind}`)}</span>
        </div>
      )}

      <div className="grid gap-2">
        <Label>{t("routing.behavior")}</Label>
        <Select value={implementation} onValueChange={setImplementation}>
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            {IMPLEMENTATIONS.map((impl) => (
              <SelectItem key={impl} value={impl}>{t(`implementation.${impl}`)}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {implementation === "transform_to" && (
        <div className="grid gap-2">
          <Label>{t("routing.destKind")}</Label>
          <Select value={destKind} onValueChange={setDestKind}>
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              {DEST_KINDS.map((k) => (
                <SelectItem key={k} value={k}>{t(`protocolKind.${k}`)}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}

      {formError && <p className="text-sm text-destructive">{formError}</p>}
      <Button type="submit" disabled={mutation.isPending}>{t("common.save")}</Button>
    </form>
  );
}
