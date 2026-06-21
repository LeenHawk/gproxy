import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { upsertOrg, type Org } from "@/api/identity";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";

interface OrgFormProps {
  /** undefined = create */
  org?: Org;
  onSaved: (saved: Org) => void;
}

export function OrgForm({ org, onSaved }: OrgFormProps) {
  const { t } = useTranslation("identity");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const editing = org !== undefined;

  const [name, setName] = useState(org?.name ?? "");
  const [description, setDescription] = useState(org?.description ?? "");
  const [enabled, setEnabled] = useState(org?.enabled ?? true);
  const [formError, setFormError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: () => {
      if (!name.trim()) throw new ApiError(0, "bad_request", t("orgs.name") + " is required");
      return upsertOrg({
        id: org?.id ?? null,
        name: name.trim(),
        description: description.trim() === "" ? null : description.trim(),
        enabled,
      });
    },
    onSuccess: (saved) => {
      void queryClient.invalidateQueries({ queryKey: ["orgs"] });
      toast.success(tc("actions.save"));
      onSaved(saved);
    },
    onError: (error) => {
      setFormError(error instanceof ApiError ? error.message : String(error));
    },
  });

  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        setFormError(null);
        mutation.mutate();
      }}
    >
      <div className="grid gap-2">
        <Label htmlFor="org-name">{t("orgs.name")}</Label>
        <Input id="org-name" value={name} onChange={(e) => setName(e.target.value)} required />
      </div>
      <div className="grid gap-2">
        <Label htmlFor="org-desc">{t("orgs.description")}</Label>
        <Input id="org-desc" value={description} onChange={(e) => setDescription(e.target.value)} />
      </div>
      <div className="flex items-center justify-between">
        <Label htmlFor="org-enabled">{t("orgs.enabled")}</Label>
        <Switch id="org-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>
      {formError && <p className="text-sm text-destructive">{formError}</p>}
      <Button type="submit" disabled={mutation.isPending}>
        {editing ? t("orgs.edit") : t("orgs.create")}
      </Button>
    </form>
  );
}
