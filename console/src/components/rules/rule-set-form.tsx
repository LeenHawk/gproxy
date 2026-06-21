import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { upsertRuleSet, type RuleSet } from "@/api/rules";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";

interface Props {
  ruleSet?: RuleSet;
  onSaved?: (rs: RuleSet) => void;
}

export function RuleSetForm({ ruleSet, onSaved }: Props) {
  const { t } = useTranslation("rules");
  const qc = useQueryClient();

  const [name, setName] = useState(ruleSet?.name ?? "");
  const [enabled, setEnabled] = useState(ruleSet?.enabled ?? true);
  const [description, setDescription] = useState(ruleSet?.description ?? "");
  const [formError, setFormError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: () => {
      if (!name.trim()) throw new ApiError(0, "bad_request", t("validation.nameRequired"));
      return upsertRuleSet({
        id: ruleSet?.id ?? null,
        name: name.trim(),
        enabled,
        description: description.trim() || null,
      });
    },
    onSuccess: (result) => {
      void qc.invalidateQueries({ queryKey: ["rule-sets"] });
      toast.success(t("common.save"));
      setFormError(null);
      onSaved?.(result);
    },
    onError: (e) => {
      setFormError(e instanceof ApiError ? e.message : String(e));
    },
  });

  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => { e.preventDefault(); setFormError(null); mutation.mutate(); }}
    >
      <div className="grid gap-1">
        <Label htmlFor="rs-name">{t("ruleSet.name")}</Label>
        <Input
          id="rs-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
        />
      </div>
      <div className="flex items-center justify-between gap-4">
        <Label htmlFor="rs-enabled">{t("ruleSet.enabled")}</Label>
        <Switch id="rs-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>
      <div className="grid gap-1">
        <Label htmlFor="rs-desc">{t("ruleSet.description")}</Label>
        <Textarea
          id="rs-desc"
          rows={3}
          value={description}
          onChange={(e) => setDescription(e.target.value)}
        />
      </div>
      {formError && (
        <p role="alert" className="text-sm text-destructive">{formError}</p>
      )}
      <Button type="submit" disabled={mutation.isPending}>
        {mutation.isPending ? "…" : t("common.save")}
      </Button>
    </form>
  );
}
