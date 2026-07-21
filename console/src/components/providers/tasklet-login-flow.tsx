import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { loginFlowComplete, loginFlowStart, type LoginStartResponse } from "@/api/login-flows";
import type { CredentialView } from "@/api/credentials";
import type { Provider } from "@/api/providers";
import { validateCallbackUrl } from "@/lib/oauth-input";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";

type TaskletMethod = "email" | "google" | "microsoft";

interface TaskletLoginFlowProps {
  provider: Provider;
  credLabel: string;
  onDone: (credential: CredentialView) => void;
}

export function TaskletLoginFlow(props: TaskletLoginFlowProps) {
  const { t } = useTranslation("providers");
  const [method, setMethod] = useState<TaskletMethod>("email");
  return (
    <div className="grid gap-4">
      <div className="grid gap-2">
        <Label>{t("wizard.taskletMethod")}</Label>
        <Tabs value={method} onValueChange={(value) => setMethod(value as TaskletMethod)}>
          <TabsList>
            <TabsTrigger value="email">{t("wizard.taskletMethods.email")}</TabsTrigger>
            <TabsTrigger value="google">Google</TabsTrigger>
            <TabsTrigger value="microsoft">Microsoft</TabsTrigger>
          </TabsList>
        </Tabs>
      </div>
      {method === "email"
        ? <EmailPinFlow key="email" {...props} />
        : <SocialFlow key={method} {...props} method={method} />}
    </div>
  );
}

function EmailPinFlow({ provider, credLabel, onDone }: TaskletLoginFlowProps) {
  const { t } = useTranslation("providers");
  const [email, setEmail] = useState("");
  const [pin, setPin] = useState("");
  const [session, setSession] = useState<LoginStartResponse | null>(null);
  const start = useMutation({
    mutationFn: () => loginFlowStart({
      channel: provider.channel, provider_id: provider.id,
      params: { auth_method: "email", email: email.trim() },
    }),
    onSuccess: (response) => { setPin(""); setSession(response); },
  });
  const complete = useMutation({
    mutationFn: () => session === null ? Promise.reject(new Error("no session")) : loginFlowComplete({
      login_session_id: session.login_session_id, code: pin.trim(), provider_id: provider.id,
      ...(credLabel.trim() !== "" ? { name: credLabel.trim() } : {}),
    }),
    onSuccess: onDone,
    onError: () => setSession(null),
  });

  if (session === null) return (
    <div className="grid gap-4">
      <div className="grid gap-2">
        <Label htmlFor="tasklet-email">{t("wizard.taskletEmail")}</Label>
        <Input id="tasklet-email" type="email" autoComplete="email" value={email}
          onChange={(event) => setEmail(event.target.value)} />
        <p className="text-xs text-muted-foreground">{t("wizard.taskletEmailHint")}</p>
      </div>
      {(start.isError || complete.isError) && <LoginFailed />}
      <Button onClick={() => { complete.reset(); start.mutate(); }}
        disabled={!email.includes("@") || start.isPending}>
        {start.isPending ? t("wizard.starting") : t("wizard.taskletSendPin")}
      </Button>
    </div>
  );

  return (
    <div className="grid gap-4">
      <div className="grid gap-2">
        <Label htmlFor="tasklet-pin">{t("wizard.taskletPin")}</Label>
        <Input id="tasklet-pin" inputMode="numeric" autoComplete="one-time-code" value={pin}
          onChange={(event) => setPin(event.target.value)} />
        <p className="text-xs text-muted-foreground">{t("wizard.taskletPinHint", { email: email.trim() })}</p>
      </div>
      {(start.isError || complete.isError) && <LoginFailed />}
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

function SocialFlow({ provider, credLabel, onDone, method }: TaskletLoginFlowProps & { method: Exclude<TaskletMethod, "email"> }) {
  const { t } = useTranslation("providers");
  const storageKey = `gproxy:tasklet-login:${provider.id}:${method}`;
  const [callback, setCallback] = useState("");
  const [session, setSession] = useState<LoginStartResponse | null>(() => loadSession(storageKey));
  const start = useMutation({
    mutationFn: () => loginFlowStart({
      channel: provider.channel, provider_id: provider.id, params: { auth_method: method },
    }),
    onSuccess: (response) => {
      saveSession(storageKey, response);
      setSession(response);
    },
  });
  const complete = useMutation({
    mutationFn: () => session === null ? Promise.reject(new Error("no session")) : loginFlowComplete({
      login_session_id: session.login_session_id, code: callback.trim(), provider_id: provider.id,
      ...(credLabel.trim() !== "" ? { name: credLabel.trim() } : {}),
    }),
    onSuccess: (credential) => { clearSession(storageKey); onDone(credential); },
    onError: () => { clearSession(storageKey); setSession(null); },
  });
  const callbackValid = session !== null && validateTaskletCallback(callback, session.authorize_url);

  if (session === null) return (
    <div className="grid gap-4">
      <p className="text-sm text-muted-foreground">{t("wizard.taskletSocialIntro")}</p>
      {(start.isError || complete.isError) && <LoginFailed />}
      <Button onClick={() => { complete.reset(); start.mutate(); }} disabled={start.isPending}>
        {start.isPending ? t("wizard.starting") : t("wizard.start")}
      </Button>
    </div>
  );

  return (
    <div className="grid gap-4">
      <p className="text-sm text-muted-foreground">{t("wizard.taskletSocialSteps")}</p>
      <Button asChild><a href={session.authorize_url}>{t("wizard.taskletOpenSocial")}</a></Button>
      <div className="grid gap-2">
        <Label htmlFor={`tasklet-${method}-callback`}>{t("wizard.pasteLabel")}</Label>
        <Textarea id={`tasklet-${method}-callback`} rows={3} value={callback} spellCheck={false}
          onChange={(event) => setCallback(event.target.value)} />
        <p className="text-xs text-muted-foreground">{t("wizard.taskletSocialHint")}</p>
      </div>
      {complete.isError && <LoginFailed />}
      <Button onClick={() => complete.mutate()} disabled={!callbackValid || complete.isPending}>
        {complete.isPending ? t("wizard.completing") : t("wizard.complete")}
      </Button>
    </div>
  );
}

function loadSession(key: string): LoginStartResponse | null {
  try {
    const stored = JSON.parse(sessionStorage.getItem(key) ?? "null") as
      { response?: LoginStartResponse; createdAt?: number } | null;
    if (stored?.response && Date.now() - (stored.createdAt ?? 0) < 10 * 60 * 1000) return stored.response;
    clearSession(key);
  } catch { /* Browser storage can be disabled. */ }
  return null;
}

function saveSession(key: string, response: LoginStartResponse) {
  try { sessionStorage.setItem(key, JSON.stringify({ response, createdAt: Date.now() })); }
  catch { /* The in-memory flow still works if browser storage is disabled. */ }
}

function clearSession(key: string) {
  try { sessionStorage.removeItem(key); } catch { /* Browser storage can be disabled. */ }
}

function validateTaskletCallback(value: string, authorizeUrl: string): boolean {
  if (!validateCallbackUrl(value, authorizeUrl)) return false;
  try {
    const url = new URL(value.trim());
    return url.origin === "https://tasklet.ai" && url.pathname === "/oauth2callback";
  } catch { return false; }
}

function LoginFailed() {
  const { t } = useTranslation("providers");
  return <p className="text-sm text-destructive">{t("wizard.failed")}</p>;
}
