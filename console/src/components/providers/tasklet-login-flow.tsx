import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { loginFlowComplete, loginFlowStart, type LoginStartResponse } from "@/api/login-flows";
import type { CredentialView } from "@/api/credentials";
import type { Provider } from "@/api/providers";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

interface TaskletLoginFlowProps {
  provider: Provider;
  credLabel: string;
  onDone: (credential: CredentialView) => void;
}

export function TaskletLoginFlow({ provider, credLabel, onDone }: TaskletLoginFlowProps) {
  const { t } = useTranslation("providers");
  const [email, setEmail] = useState("");
  const [pin, setPin] = useState("");
  const [session, setSession] = useState<LoginStartResponse | null>(null);

  const start = useMutation({
    mutationFn: () => loginFlowStart({
      channel: provider.channel,
      provider_id: provider.id,
      params: { email: email.trim() },
    }),
    onSuccess: (response) => { setPin(""); setSession(response); },
  });
  const complete = useMutation({
    mutationFn: () => {
      if (session === null) return Promise.reject(new Error("no session"));
      return loginFlowComplete({
        login_session_id: session.login_session_id,
        code: pin.trim(),
        provider_id: provider.id,
        ...(credLabel.trim() !== "" ? { name: credLabel.trim() } : {}),
      });
    },
    onSuccess: onDone,
    onError: () => setSession(null),
  });

  if (session === null) {
    return (
      <div className="grid gap-4">
        <div className="grid gap-2">
          <Label htmlFor="tasklet-email">{t("wizard.taskletEmail")}</Label>
          <Input id="tasklet-email" type="email" autoComplete="email" value={email}
            onChange={(event) => setEmail(event.target.value)} />
          <p className="text-xs text-muted-foreground">{t("wizard.taskletEmailHint")}</p>
        </div>
        {(start.isError || complete.isError) && <p className="text-sm text-destructive">{t("wizard.failed")}</p>}
        <Button onClick={() => { complete.reset(); start.mutate(); }}
          disabled={!email.includes("@") || start.isPending}>
          {start.isPending ? t("wizard.starting") : t("wizard.taskletSendPin")}
        </Button>
      </div>
    );
  }

  return (
    <div className="grid gap-4">
      <div className="grid gap-2">
        <Label htmlFor="tasklet-pin">{t("wizard.taskletPin")}</Label>
        <Input id="tasklet-pin" inputMode="numeric" autoComplete="one-time-code" value={pin}
          onChange={(event) => setPin(event.target.value)} />
        <p className="text-xs text-muted-foreground">{t("wizard.taskletPinHint", { email: email.trim() })}</p>
      </div>
      {(start.isError || complete.isError) && <p className="text-sm text-destructive">{t("wizard.failed")}</p>}
      <Button onClick={() => complete.mutate()} disabled={pin.trim() === "" || complete.isPending}>
        {complete.isPending ? t("wizard.completing") : t("wizard.complete")}
      </Button>
      <Button variant="outline" onClick={() => { complete.reset(); start.mutate(); }}
        disabled={start.isPending || complete.isPending}>
        {start.isPending ? t("wizard.starting") : t("wizard.taskletResendPin")}
      </Button>
    </div>
  );
}
