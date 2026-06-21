import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { upsertProvider, type Provider } from "@/api/providers";
import { ApiError } from "@/api/http";
import { CHANNELS } from "@/lib/channel-meta";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectTrigger, SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  SettingsFields, type SettingsState, initSettingsState, assembleSettings,
} from "./settings-fields";
import { TlsFingerprintField } from "./tls-fingerprint-field";

interface ProviderFormProps {
  /** undefined = create */
  provider?: Provider;
  onSaved: (saved: Provider) => void;
}

const STRATEGIES = ["round_robin", "sticky"] as const;

export function ProviderForm({ provider, onSaved }: ProviderFormProps) {
  const { t } = useTranslation("providers");
  const queryClient = useQueryClient();
  const editing = provider !== undefined;

  const [name, setName] = useState(provider?.name ?? "");
  const [label, setLabel] = useState(provider?.label ?? "");
  const [channel, setChannel] = useState(provider?.channel ?? "openai");
  const [strategy, setStrategy] = useState(provider?.credential_strategy ?? "round_robin");
  const [proxyUrl, setProxyUrl] = useState(provider?.proxy_url ?? "");
  const [enabled, setEnabled] = useState(provider?.enabled ?? true);
  const [settings, setSettings] = useState<SettingsState>(() =>
    initSettingsState(provider?.settings_json),
  );
  const [tls, setTls] = useState<unknown>(provider?.tls_fingerprint ?? null);
  const [formError, setFormError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: () => {
      if (!name.trim()) throw new ApiError(0, "bad_request", t("form.required"));
      if (channel === "custom" && !settings.baseUrl.trim()) {
        throw new ApiError(0, "bad_request", t("form.baseUrlRequired"));
      }

      const settings_json = assembleSettings(provider?.settings_json, settings, channel);

      // tls_fingerprint: send blob to set/update; omit (absent) to clear → backend NULL
      const tlsPayload: { tls_fingerprint?: unknown } = {};
      if (tls != null) {
        tlsPayload.tls_fingerprint = tls;
      }

      return upsertProvider({
        id: provider?.id ?? null,
        name: name.trim(),
        channel,
        label: label.trim() === "" ? null : label.trim(),
        settings_json,
        credential_strategy: strategy,
        proxy_url: proxyUrl.trim() === "" ? null : proxyUrl.trim(),
        ...tlsPayload,
        enabled,
      });
    },
    onSuccess: (saved) => {
      void queryClient.invalidateQueries({ queryKey: ["providers"] });
      toast.success(t("form.saved"));
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
        <Label htmlFor="p-name">{t("fields.name")}</Label>
        <Input id="p-name" value={name} onChange={(e) => setName(e.target.value)} required />
      </div>
      <div className="grid gap-2">
        <Label htmlFor="p-label">{t("fields.label")}</Label>
        <Input id="p-label" value={label} onChange={(e) => setLabel(e.target.value)} />
      </div>
      <div className="grid gap-2">
        <Label>{t("fields.channel")}</Label>
        <Select value={channel} disabled={editing} onValueChange={(v) => { setChannel(v); setSettings(initSettingsState(provider?.settings_json)); }}>
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            {(["api_key", "oauth_tokens", "service_account", "github_token"] as const).map((family) => {
              const group = CHANNELS.filter((c) => c.family === family);
              if (group.length === 0) return null;
              return (
                <SelectGroup key={family}>
                  <SelectLabel>{t(`family.${family}`)}</SelectLabel>
                  {group.map((c) => (
                    <SelectItem key={c.id} value={c.id}>{c.id}</SelectItem>
                  ))}
                </SelectGroup>
              );
            })}
          </SelectContent>
        </Select>
        {editing && <p className="text-xs text-muted-foreground">{t("form.channelLocked")}</p>}
      </div>
      <div className="grid gap-2">
        <Label>{t("fields.strategy")}</Label>
        <Select value={strategy} onValueChange={setStrategy}>
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            {STRATEGIES.map((s) => (
              <SelectItem key={s} value={s}>{t(`strategy.${s}`)}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="grid gap-2">
        <Label htmlFor="p-proxy">{t("fields.proxyUrl")}</Label>
        <Input id="p-proxy" value={proxyUrl} onChange={(e) => setProxyUrl(e.target.value)} placeholder="socks5://… / http://…" />
      </div>

      {/* Structured settings — no raw JSON */}
      <SettingsFields
        channel={channel}
        state={settings}
        onChange={(next) => setSettings((prev) => ({ ...prev, ...next }))}
      />

      {/* TLS fingerprint — popover preset picker */}
      <TlsFingerprintField value={tls} onChange={setTls} label={t("fields.tlsProfile")} />

      <div className="flex items-center justify-between">
        <Label htmlFor="p-enabled">{t("fields.enabled")}</Label>
        <Switch id="p-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>
      {formError && <p className="text-sm text-destructive">{formError}</p>}
      <Button type="submit" disabled={mutation.isPending}>
        {editing ? t("form.edit") : t("form.create")}
      </Button>
    </form>
  );
}
