import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { changePassword } from "@/api/portal";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

interface FormState {
  current: string;
  next: string;
  confirm: string;
}

export function ChangePasswordForm() {
  const { t } = useTranslation("portal");
  const { t: tc } = useTranslation("common");

  const [f, setF] = useState<FormState>({ current: "", next: "", confirm: "" });
  const set = <K extends keyof FormState>(k: K) =>
    (v: string) => setF((prev) => ({ ...prev, [k]: v }));

  const [formError, setFormError] = useState<string | null>(null);

  const mismatchMsg = t("account.mismatch");
  const tooShortMsg = t("account.tooShort");
  const nextHasError = formError === tooShortMsg;
  const confirmHasError = formError === mismatchMsg;

  const mutation = useMutation({
    mutationFn: async () => {
      if (f.next.length < 12) throw new ApiError(0, "bad_request", tooShortMsg);
      if (f.next !== f.confirm) throw new ApiError(0, "bad_request", mismatchMsg);
      return changePassword(f.current, f.next);
    },
    onSuccess: () => {
      toast.success(t("account.changed"));
      setFormError(null);
      setF({ current: "", next: "", confirm: "" });
    },
    onError: (e) => {
      setFormError(e instanceof ApiError ? e.message : String(e));
    },
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">{t("account.changePassword")}</CardTitle>
      </CardHeader>
      <CardContent>
        <form
          className="grid gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            setFormError(null);
            mutation.mutate();
          }}
        >
          {formError && (
            <div
              role="alert"
              id="change-pw-err"
              className="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive"
            >
              {formError}
            </div>
          )}

          <div className="grid gap-1">
            <Label htmlFor="pw-current">{t("account.currentPassword")}</Label>
            <Input
              id="pw-current"
              type="password"
              value={f.current}
              onChange={(e) => set("current")(e.target.value)}
              autoComplete="current-password"
              required
            />
          </div>

          <div className="grid gap-1">
            <Label htmlFor="pw-new">{t("account.newPassword")}</Label>
            <Input
              id="pw-new"
              type="password"
              value={f.next}
              onChange={(e) => set("next")(e.target.value)}
              autoComplete="new-password"
              aria-invalid={nextHasError ? true : undefined}
              aria-describedby={nextHasError ? "change-pw-err" : undefined}
              required
            />
          </div>

          <div className="grid gap-1">
            <Label htmlFor="pw-confirm">{t("account.confirmPassword")}</Label>
            <Input
              id="pw-confirm"
              type="password"
              value={f.confirm}
              onChange={(e) => set("confirm")(e.target.value)}
              autoComplete="new-password"
              aria-invalid={confirmHasError ? true : undefined}
              aria-describedby={confirmHasError ? "change-pw-err" : undefined}
              required
            />
          </div>

          <div>
            <Button type="submit" disabled={mutation.isPending}>
              {mutation.isPending ? "…" : tc("actions.save")}
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  );
}
