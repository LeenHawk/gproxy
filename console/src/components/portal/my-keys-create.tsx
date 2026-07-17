import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { createMyKey, myKeysQuery } from "@/api/portal";
import { ApiError } from "@/api/http";
import { EntityDialog } from "@/components/entity-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function MyKeysCreate({ open, onOpenChange }: Props) {
  const { t } = useTranslation("portal");
  const queryClient = useQueryClient();

  const [label, setLabel] = useState("");

  const generate = useMutation({
    mutationFn: () => createMyKey(label.trim() || null),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: myKeysQuery.queryKey });
      onOpenChange(false);
      setLabel("");
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
  });

  return (
    <EntityDialog open={open} onOpenChange={onOpenChange} title={t("keys.add")}>
      <form
        className="grid gap-4"
        onSubmit={(e) => { e.preventDefault(); generate.mutate(); }}
      >
        <div className="grid gap-2">
          <Label htmlFor="portal-key-label">{t("keys.label")}</Label>
          <Input
            id="portal-key-label"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder={t("keys.label")}
          />
        </div>
        <Button type="submit" disabled={generate.isPending}>{t("keys.add")}</Button>
      </form>
    </EntityDialog>
  );
}
