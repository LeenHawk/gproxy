import { useEffect, useRef, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ExternalLink } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  cookieLogin, deviceStart, devicePoll, loginFlowComplete, loginFlowStart,
  type DeviceStartResponse, type LoginStartResponse,
} from "@/api/login-flows";
import type { CredentialView } from "@/api/credentials";
import { channelMeta, type LoginMode } from "@/lib/channel-meta";
import { extractSessionKey, validateCallbackUrl } from "@/lib/oauth-input";
import type { Provider } from "@/api/providers";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";

interface OAuthWizardProps {
  provider: Provider;
  onDone: (credential: CredentialView) => void;
}

export function OAuthWizard({ provider, onDone }: OAuthWizardProps) {
  const { t } = useTranslation("providers");
  const queryClient = useQueryClient();
  const meta = channelMeta(provider.channel);
  const modes = meta?.loginModes ?? [];
  const [mode, setMode] = useState<LoginMode>(modes[0] ?? "authcode");
  const [credLabel, setCredLabel] = useState("");
  // Kiro has four credential methods that span both the device and authcode
  // flows, so it gets its own picker instead of the generic mode tabs.
  const isKiro = provider.channel === "kiro";
  // Geminicli's two modes are both authcode but differ in redirect + paste UI
  // (callback URL vs bare code), so it gets a dedicated toggle.
  const isGemini = provider.channel === "geminicli";

  const finish = (credential: CredentialView) => {
    void queryClient.invalidateQueries({ queryKey: ["providers", provider.id, "credentials"] });
    onDone(credential);
  };

  return (
    <div className="grid gap-4">
      {!isKiro && !isGemini && modes.length > 1 && (
        <Tabs value={mode} onValueChange={(v) => setMode(v as LoginMode)}>
          <TabsList>
            {modes.map((m) => (
              <TabsTrigger key={m} value={m}>{t(`wizard.mode.${m}`)}</TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
      )}
      <div className="grid gap-2">
        <Label htmlFor="w-name">{t("wizard.credName")}</Label>
        <Input id="w-name" value={credLabel} onChange={(e) => setCredLabel(e.target.value)} />
      </div>
      {isKiro ? (
        <KiroWizard provider={provider} credLabel={credLabel} onDone={finish} />
      ) : isGemini ? (
        <GeminiWizard provider={provider} credLabel={credLabel} onDone={finish} />
      ) : (
        <>
          {mode === "authcode" && <AuthcodeFlow provider={provider} credLabel={credLabel} onDone={finish} startParams={meta?.loginParams} />}
          {mode === "device" && <DeviceFlow provider={provider} credLabel={credLabel} onDone={finish} />}
          {mode === "cookie" && <CookieFlow provider={provider} credLabel={credLabel} onDone={finish} />}
        </>
      )}
    </div>
  );
}

interface FlowProps {
  provider: Provider;
  credLabel: string;
  onDone: (credential: CredentialView) => void;
}

// ── Kiro: the four login methods kiro-cli's `login` menu offers ────────────────
// "Use with Google / GitHub / Builder ID / Your Organization". GitHub / Google →
// device flow (params {login_provider}); Builder ID / Your-Organization(idc) →
// authcode + PKCE. (external_idp is a real fig_auth kind but is portal-config
// driven, NOT a menu item — so it is reachable via the API, not surfaced here.)
const KIRO_METHODS = ["github", "google", "builderId", "idc"] as const;
type KiroMethod = (typeof KIRO_METHODS)[number];

function KiroWizard({ provider, credLabel, onDone }: FlowProps) {
  const { t } = useTranslation("providers");
  const [method, setMethod] = useState<KiroMethod>("github");
  const [startUrl, setStartUrl] = useState("");
  const [region, setRegion] = useState("us-east-1");

  const isSocial = method === "github" || method === "google";

  const authParams: Record<string, unknown> =
    method === "idc"
      ? {
          auth_method: "idc",
          region: region.trim() || "us-east-1",
          ...(startUrl.trim() !== "" ? { start_url: startUrl.trim() } : {}),
        }
      : { auth_method: "builderId" };

  const idcMissing = method === "idc" && startUrl.trim() === "";

  // GitHub / Google are proper nouns (same in every locale); the rest are translated.
  const methodLabel = (m: KiroMethod): string =>
    m === "github" ? "GitHub" : m === "google" ? "Google" : t(`wizard.kiroMethods.${m}`);

  return (
    <div className="grid gap-4">
      <div className="grid gap-2">
        <Label>{t("wizard.kiroMethod")}</Label>
        <Select value={method} onValueChange={(v) => setMethod(v as KiroMethod)}>
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            {KIRO_METHODS.map((m) => (
              <SelectItem key={m} value={m}>{methodLabel(m)}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {isSocial && (
        <DeviceFlow key={method} provider={provider} credLabel={credLabel} onDone={onDone}
          params={{ login_provider: method }} />
      )}

      {method === "builderId" && (
        <AuthcodeFlow key="builderId" provider={provider} credLabel={credLabel} onDone={onDone}
          startParams={authParams} />
      )}

      {method === "idc" && (
        <>
          <div className="grid gap-2">
            <Label htmlFor="kiro-start">{t("wizard.kiroStartUrl")}</Label>
            <Input id="kiro-start" value={startUrl} onChange={(e) => setStartUrl(e.target.value)}
              placeholder="https://….awsapps.com/start" />
            <p className="text-xs text-muted-foreground">{t("wizard.kiroStartUrlHint")}</p>
          </div>
          <div className="grid gap-2">
            <Label htmlFor="kiro-region">{t("wizard.kiroRegion")}</Label>
            <Input id="kiro-region" value={region} onChange={(e) => setRegion(e.target.value)} />
          </div>
          <AuthcodeFlow key="idc" provider={provider} credLabel={credLabel} onDone={onDone}
            startParams={authParams} disabled={idcMissing} />
        </>
      )}
    </div>
  );
}

// ── Geminicli: two authcode modes (callback URL vs bare code) ──────────────────
// callback URL → loopback redirect (code_only:false), paste the ?code=&state= URL.
// code only   → codeassist redirect (code_only:true), paste the bare code Google shows.
function GeminiWizard({ provider, credLabel, onDone }: FlowProps) {
  const { t } = useTranslation("providers");
  const [mode, setMode] = useState<"callback" | "code">("callback");

  return (
    <div className="grid gap-4">
      <div className="grid gap-2">
        <Label>{t("wizard.geminiMode")}</Label>
        <Tabs value={mode} onValueChange={(v) => setMode(v as "callback" | "code")}>
          <TabsList>
            <TabsTrigger value="callback">{t("wizard.geminiCallback")}</TabsTrigger>
            <TabsTrigger value="code">{t("wizard.geminiCodeOnly")}</TabsTrigger>
          </TabsList>
        </Tabs>
      </div>
      {mode === "callback" && (
        <AuthcodeFlow key="callback" provider={provider} credLabel={credLabel} onDone={onDone}
          startParams={{ code_only: false }} />
      )}
      {mode === "code" && (
        <CodeOnlyFlow key="code" provider={provider} credLabel={credLabel} onDone={onDone}
          startParams={{ code_only: true }} />
      )}
    </div>
  );
}

// A code-only authcode flow: open the authorize URL, then paste the BARE code the
// provider shows (no callback URL / state — the backend skips the state check).
function CodeOnlyFlow({
  provider, credLabel, onDone, startParams,
}: FlowProps & { startParams?: Record<string, unknown> }) {
  const { t } = useTranslation("providers");
  const [session, setSession] = useState<LoginStartResponse | null>(null);
  const [pasted, setPasted] = useState("");

  const start = useMutation({
    mutationFn: () => loginFlowStart({ channel: provider.channel, provider_id: provider.id, params: startParams }),
    onSuccess: (resp) => {
      setSession(resp);
      window.open(resp.authorize_url, "_blank", "noopener");
    },
  });

  const complete = useMutation({
    mutationFn: () => {
      if (session === null) return Promise.reject(new Error("no session"));
      return loginFlowComplete({
        login_session_id: session.login_session_id,
        code: pasted.trim(),
        provider_id: provider.id,
        ...(credLabel.trim() !== "" ? { name: credLabel.trim() } : {}),
      });
    },
    onSuccess: onDone,
  });

  if (session === null) {
    return (
      <div className="grid gap-4">
        {start.isError && <p className="text-sm text-destructive">{t("wizard.failed")}</p>}
        <Button onClick={() => start.mutate()} disabled={start.isPending}>
          {start.isPending ? t("wizard.starting") : t("wizard.start")}
        </Button>
      </div>
    );
  }

  return (
    <div className="grid gap-4">
      <Button variant="outline" onClick={() => window.open(session.authorize_url, "_blank", "noopener")}>
        <ExternalLink className="size-4" />
        {t("wizard.openAuthorize")}
      </Button>
      <div className="grid gap-2">
        <Label htmlFor="w-code">{t("wizard.pasteCodeLabel")}</Label>
        <Textarea id="w-code" rows={2} value={pasted} spellCheck={false}
          onChange={(e) => setPasted(e.target.value)} />
        <p className="text-xs text-muted-foreground">{t("wizard.pasteCodeHint")}</p>
      </div>
      {complete.isError && <p className="text-sm text-destructive">{t("wizard.failed")}</p>}
      <Button onClick={() => complete.mutate()} disabled={pasted.trim() === "" || complete.isPending}>
        {complete.isPending ? t("wizard.completing") : t("wizard.complete")}
      </Button>
    </div>
  );
}

function AuthcodeFlow({
  provider, credLabel, onDone, startParams, disabled,
}: FlowProps & { startParams?: Record<string, unknown>; disabled?: boolean }) {
  const { t } = useTranslation("providers");
  const [session, setSession] = useState<LoginStartResponse | null>(null);
  const [pasted, setPasted] = useState("");
  const [pasteTouched, setPasteTouched] = useState(false);

  const start = useMutation({
    mutationFn: () => loginFlowStart({ channel: provider.channel, provider_id: provider.id, params: startParams }),
    onSuccess: (resp) => {
      setSession(resp);
      window.open(resp.authorize_url, "_blank", "noopener");
    },
  });

  const complete = useMutation({
    mutationFn: () => {
      if (session === null) return Promise.reject(new Error("no session"));
      return loginFlowComplete({
        login_session_id: session.login_session_id,
        callback_url: pasted.trim(),
        provider_id: provider.id,
        ...(credLabel.trim() !== "" ? { name: credLabel.trim() } : {}),
      });
    },
    onSuccess: onDone,
  });

  const pasteValid = session !== null && validateCallbackUrl(pasted, session.authorize_url);

  if (session === null) {
    return (
      <div className="grid gap-4">
        {start.isError && <p className="text-sm text-destructive">{t("wizard.failed")}</p>}
        <Button onClick={() => start.mutate()} disabled={start.isPending || disabled === true}>
          {start.isPending ? t("wizard.starting") : t("wizard.start")}
        </Button>
      </div>
    );
  }

  return (
    <div className="grid gap-4">
      <Button variant="outline" onClick={() => window.open(session.authorize_url, "_blank", "noopener")}>
        <ExternalLink className="size-4" />
        {t("wizard.openAuthorize")}
      </Button>
      <div className="grid gap-2">
        <Label htmlFor="w-cb">{t("wizard.pasteLabel")}</Label>
        <Textarea id="w-cb" rows={3} value={pasted} spellCheck={false}
          onChange={(e) => setPasted(e.target.value)} onBlur={() => setPasteTouched(true)} />
        <p className={pasteTouched && pasted.trim() !== "" && !pasteValid ? "text-xs text-destructive" : "text-xs text-muted-foreground"}>
          {pasteTouched && pasted.trim() !== "" && !pasteValid ? t("wizard.pasteInvalid") : t("wizard.pasteHint")}
        </p>
      </div>
      {complete.isError && <p className="text-sm text-destructive">{t("wizard.failed")}</p>}
      <Button onClick={() => complete.mutate()} disabled={!pasteValid || complete.isPending}>
        {complete.isPending ? t("wizard.completing") : t("wizard.complete")}
      </Button>
    </div>
  );
}

function DeviceFlow({
  provider, credLabel, onDone, params,
}: FlowProps & { params?: Record<string, unknown> }) {
  const { t } = useTranslation("providers");
  const [session, setSession] = useState<DeviceStartResponse | null>(null);
  const [failed, setFailed] = useState(false);
  const stopped = useRef(false);
  // Keep the latest onDone without retriggering the poll effect (parent re-renders
  // recreate the closure; resetting the timer on each render would stall polling).
  const onDoneRef = useRef(onDone);
  onDoneRef.current = onDone;

  const start = useMutation({
    mutationFn: () =>
      deviceStart({
        channel: provider.channel,
        provider_id: provider.id,
        ...(credLabel.trim() !== "" ? { name: credLabel.trim() } : {}),
        ...(params !== undefined ? { params } : {}),
      }),
    onSuccess: (resp) => { setFailed(false); setSession(resp); },
  });

  useEffect(() => {
    if (session === null) return;
    stopped.current = false;
    let timer: ReturnType<typeof setTimeout>;
    const tick = async () => {
      try {
        const resp = await devicePoll(session.login_session_id);
        if (stopped.current) return;
        if (resp.status === "ready") {
          onDoneRef.current(resp.credential);
          return;
        }
        timer = setTimeout(() => void tick(), Math.max(session.interval_secs, 2) * 1000);
      } catch {
        if (!stopped.current) { setFailed(true); setSession(null); }
      }
    };
    timer = setTimeout(() => void tick(), Math.max(session.interval_secs, 2) * 1000);
    return () => { stopped.current = true; clearTimeout(timer); };
  }, [session]);

  if (session === null) {
    return (
      <div className="grid gap-4">
        {(failed || start.isError) && <p className="text-sm text-destructive">{t("wizard.failed")}</p>}
        <Button onClick={() => start.mutate()} disabled={start.isPending}>
          {start.isPending ? t("wizard.starting") : t("wizard.start")}
        </Button>
      </div>
    );
  }

  return (
    <div className="grid gap-3 text-center">
      <p className="text-sm text-muted-foreground">{t("wizard.deviceIntro")}</p>
      <a className="text-sm font-medium underline" href={session.verification_url} target="_blank" rel="noopener noreferrer">
        {session.verification_url}
      </a>
      <p className="font-mono text-2xl font-semibold tracking-widest">{session.user_code}</p>
      <p className="text-xs text-muted-foreground">{t("wizard.waiting")}</p>
    </div>
  );
}

function CookieFlow({ provider, credLabel, onDone }: FlowProps) {
  const { t } = useTranslation("providers");
  const [pasted, setPasted] = useState("");
  const [touched, setTouched] = useState(false);
  const cookie = extractSessionKey(pasted);

  const mutation = useMutation({
    mutationFn: () => {
      if (cookie === null) return Promise.reject(new Error("no cookie"));
      return cookieLogin({
        channel: provider.channel,
        cookie,
        provider_id: provider.id,
        ...(credLabel.trim() !== "" ? { name: credLabel.trim() } : {}),
      });
    },
    onSuccess: onDone,
  });

  return (
    <div className="grid gap-4">
      <div className="grid gap-2">
        <Label htmlFor="w-cookie">{t("wizard.cookieLabel")}</Label>
        <Textarea id="w-cookie" rows={3} value={pasted} spellCheck={false} autoComplete="off"
          onChange={(e) => setPasted(e.target.value)} onBlur={() => setTouched(true)} />
        <p className={touched && pasted.trim() !== "" && cookie === null ? "text-xs text-destructive" : "text-xs text-muted-foreground"}>
          {touched && pasted.trim() !== "" && cookie === null ? t("wizard.cookieInvalid") : t("wizard.cookieHint")}
        </p>
      </div>
      {mutation.isError && <p className="text-sm text-destructive">{t("wizard.failed")}</p>}
      <Button onClick={() => mutation.mutate()} disabled={cookie === null || mutation.isPending}>
        {mutation.isPending ? t("wizard.completing") : t("wizard.complete")}
      </Button>
    </div>
  );
}
