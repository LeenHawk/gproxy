import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { upsertTeam, type Team } from "@/api/identity";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";

interface TeamFormProps {
  orgId: number;
  team?: Team;
  onSaved: () => void;
}

export function TeamForm({ orgId, team, onSaved }: TeamFormProps) {
  const { t } = useTranslation("identity");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const editing = team !== undefined;

  const [name, setName] = useState(team?.name ?? "");
  const [enabled, setEnabled] = useState(team?.enabled ?? true);
  const [formError, setFormError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: () => {
      if (!name.trim()) throw new ApiError(0, "bad_request", t("teams.name") + " is required");
      return upsertTeam(orgId, {
        id: team?.id ?? null,
        org_id: orgId, // enforced = URL org_id
        name: name.trim(),
        enabled,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["orgs", orgId, "teams"] });
      toast.success(tc("actions.save"));
      onSaved();
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
        <Label htmlFor="team-name">{t("teams.name")}</Label>
        <Input id="team-name" value={name} onChange={(e) => setName(e.target.value)} required />
      </div>
      <div className="flex items-center justify-between">
        <Label htmlFor="team-enabled">{t("teams.enabled")}</Label>
        <Switch id="team-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>
      {formError && <p className="text-sm text-destructive">{formError}</p>}
      <Button type="submit" disabled={mutation.isPending}>
        {editing ? t("teams.edit") : t("teams.add")}
      </Button>
    </form>
  );
}
