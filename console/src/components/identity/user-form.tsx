import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { orgsQuery, teamsQuery, upsertUser, type UserView } from "@/api/identity";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

interface UserFormProps {
  /** undefined = create */
  user?: UserView;
  onSaved: (saved: UserView) => void;
}

export function UserForm({ user, onSaved }: UserFormProps) {
  const { t } = useTranslation("identity");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const editing = user !== undefined;

  const [name, setName] = useState(user?.name ?? "");
  const [orgId, setOrgId] = useState<number | null>(user?.org_id ?? null);
  const [teamId, setTeamId] = useState<number | null>(user?.team_id ?? null);
  const [password, setPassword] = useState("");
  const [enabled, setEnabled] = useState(user?.enabled ?? true);
  const [isAdmin, setIsAdmin] = useState(user?.is_admin ?? false);
  const [formError, setFormError] = useState<string | null>(null);

  const { data: orgs } = useQuery(orgsQuery);
  const { data: teams, isPending: teamsLoading } = useQuery({
    ...teamsQuery(orgId ?? 0),
    enabled: orgId !== null,
  });

  const handleOrgChange = (val: string) => {
    if (Number(val) === orgId) return;
    setOrgId(Number(val));
    setTeamId(null); // cascade reset
  };

  const mutation = useMutation({
    mutationFn: () => {
      if (!name.trim()) throw new ApiError(0, "bad_request", t("users.name") + " is required");
      if (orgId === null) throw new ApiError(0, "bad_request", t("users.org") + " is required");
      const body = {
        id: user?.id ?? null,
        name: name.trim(),
        org_id: orgId,
        team_id: teamId,
        enabled,
        is_admin: isAdmin,
        ...(password !== "" ? { password } : {}),
      };
      return upsertUser(body);
    },
    onSuccess: (saved) => {
      void queryClient.invalidateQueries({ queryKey: ["users"] });
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
        <Label htmlFor="user-name">{t("users.name")}</Label>
        <Input id="user-name" value={name} onChange={(e) => setName(e.target.value)} required />
      </div>

      <div className="grid gap-2">
        <Label htmlFor="user-org">{t("users.org")}</Label>
        <Select value={orgId !== null ? String(orgId) : ""} onValueChange={handleOrgChange}>
          <SelectTrigger id="user-org" className="w-full">
            <SelectValue placeholder={t("users.org")} />
          </SelectTrigger>
          <SelectContent>
            {(orgs ?? []).map((org) => (
              <SelectItem key={org.id} value={String(org.id)}>{org.name}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="grid gap-2">
        <Label htmlFor="user-team">{t("users.team")}</Label>
        <Select
          value={teamId !== null ? String(teamId) : "null"}
          onValueChange={(v) => setTeamId(v === "null" ? null : Number(v))}
          disabled={orgId === null || teamsLoading}
        >
          <SelectTrigger id="user-team" className="w-full">
            <SelectValue placeholder={t("users.noTeam")} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="null">{t("users.noTeam")}</SelectItem>
            {(teams ?? []).map((team) => (
              <SelectItem key={team.id} value={String(team.id)}>{team.name}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="grid gap-2">
        <Label htmlFor="user-password">{t("users.password")}</Label>
        <Input
          id="user-password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder={editing ? t("users.passwordKeep") : t("users.passwordPolicy")}
          autoComplete="new-password"
        />
        {!editing && (
          <p className="text-xs text-muted-foreground">{t("users.passwordPolicy")}</p>
        )}
      </div>

      <div className="flex items-center justify-between">
        <Label htmlFor="user-enabled">{t("users.enabled")}</Label>
        <Switch id="user-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>

      <div className="flex items-center justify-between">
        <Label htmlFor="user-admin">{t("users.isAdmin")}</Label>
        <Switch id="user-admin" checked={isAdmin} onCheckedChange={setIsAdmin} />
      </div>

      {formError && <p className="text-sm text-destructive">{formError}</p>}

      <Button type="submit" disabled={mutation.isPending}>
        {editing ? t("users.edit") : t("users.create")}
      </Button>
    </form>
  );
}
