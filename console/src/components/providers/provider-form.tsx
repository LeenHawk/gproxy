import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { upsertProvider, type Provider } from "@/api/providers";
import { ApiError } from "@/api/http";
import { channelMeta } from "@/lib/channel-meta";
import { ensureProviderDefaultRuleSet } from "@/lib/provider-rule-set";
import { useChannelCatalog } from "@/hooks/use-channel-catalog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectTrigger, SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  SettingsFields, type SettingsState, initSettingsState, assembleSettings,
  validateSettingsState,
} from "./settings-fields";
import { TlsFingerprintField } from "./tls-fingerprint-field";
import { ProxyConnectivityTest } from "@/components/proxy-connectivity-test";

interface ProviderFormProps {
  provider?: Provider;
  onSaved: (saved: Provider) => void;
}

const STRATEGIES = ["round_robin", "sticky"] as const;

export function ProviderForm({ provider, onSaved }: ProviderFormProps) {
  const { t } = useTranslation("providers");
  const queryClient = useQueryClient();
  const catalogState = useChannelCatalog();
  const catalog = catalogState.catalog;
  const editing = provider !== undefined;

  const [name, setName] = useState(provider?.name ?? "");
  const [label, setLabel] = useState(provider?.label ?? "");
  const [channel, setChannel] = useState(provider?.channel ?? catalog[0]?.id ?? "");
  const selectedMeta = channelMeta(channel, catalog);
  const catalogMessageKey = catalogState.availability === "ready"
    ? "catalog.metadataUnavailable"
    : `catalog.${catalogState.availability}`;
  const [strategy, setStrategy] = useState(provider?.credential_strategy ?? "round_robin");
  const [proxyUrl, setProxyUrl] = useState(provider?.proxy_url ?? "");
  const [enabled, setEnabled] = useState(provider?.enabled ?? true);
  const [settings, setSettings] = useState<SettingsState>(() =>
    initSettingsState(provider?.settings_json, selectedMeta),
  );
  const [settingsMetaResolved, setSettingsMetaResolved] = useState(
    catalogState.authoritative && selectedMeta !== undefined,
  );
  const [tls, setTls] = useState<unknown>(provider?.tls_fingerprint ?? null);
  const [formError, setFormError] = useState<string | null>(null);

  useEffect(() => {
    if (editing || selectedMeta || catalogState.availability !== "ready") return;
    const next = catalog[0];
    const nextChannel = next?.id ?? "";
    if (channel === nextChannel) return;
    setChannel(nextChannel);
    setSettings(initSettingsState(undefined, next));
    setSettingsMetaResolved(next !== undefined);
  }, [catalog, catalogState.availability, channel, editing, selectedMeta]);

  useEffect(() => {
    if (settingsMetaResolved || !catalogState.authoritative || !selectedMeta) return;
    setSettings(initSettingsState(provider?.settings_json, selectedMeta));
    setSettingsMetaResolved(true);
  }, [
    catalogState.authoritative,
    channel,
    provider?.settings_json,
    selectedMeta,
    settingsMetaResolved,
  ]);

  const mutation = useMutation({
    mutationFn: () => {
      if (!catalogState.authoritative) {
        throw new ApiError(0, "bad_request", t(catalogMessageKey));
      }
      if (!name.trim()) throw new ApiError(0, "bad_request", t("form.required"));
      if (!selectedMeta) {
        throw new ApiError(0, "bad_request", t("catalog.metadataUnavailable"));
      }
      const settingsError = validateSettingsState(settings, selectedMeta);
      if (settingsError === "base_url_required") {
        throw new ApiError(0, "bad_request", t("form.baseUrlRequired"));
      }
      if (settingsError === "endpoints_required") {
        throw new ApiError(0, "bad_request", t("form.endpointsRequired"));
      }
      if (settingsError === "endpoints_invalid") {
        throw new ApiError(0, "bad_request", t("endpoints.invalid"));
      }
      if (settingsError === "circuit_breaker_invalid") {
        throw new ApiError(0, "bad_request", t("form.circuitBreakerInvalid"));
      }
      if (
        selectedMeta.source === "builtin"
        && channel === "custom"
        && !settings.baseUrl.trim()
        && settings.endpoints.length === 0
      ) {
        throw new ApiError(0, "bad_request", t("form.baseUrlOrEndpointRequired"));
      }
      if (
        selectedMeta.source === "builtin"
        && channel === "azure"
        && !settings.baseUrl.trim()
        && settings.endpoints.length === 0
      ) {
        throw new ApiError(0, "bad_request", t("form.azureBaseUrlOrEndpointRequired"));
      }
      const settings_json = assembleSettings(provider?.settings_json, settings, channel, selectedMeta);

      const tlsPayload: { tls_fingerprint?: unknown } = {};
      if (tls != null) tlsPayload.tls_fingerprint = tls;

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
      if (!editing) {
        ensureProviderDefaultRuleSet(saved.id, saved.name).catch((e) =>
          toast.error(e instanceof ApiError ? e.message : String(e)),
        );
      }
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
      {!catalogState.authoritative && (
        <p className={catalogState.availability === "error"
          ? "text-sm text-destructive"
          : "text-sm text-muted-foreground"}>
          {t(catalogMessageKey)}
        </p>
      )}
      <div className="grid gap-2">
        <Label htmlFor="p-name">{t("fields.name")}</Label>
        <Input id="p-name" value={name} onChange={(e) => setName(e.target.value)} required />
        <p className="text-xs text-muted-foreground">{t("fields.nameHint")}</p>
      </div>
      <div className="grid gap-2">
        <Label htmlFor="p-label">{t("fields.label")}</Label>
        <Input id="p-label" value={label} onChange={(e) => setLabel(e.target.value)} />
      </div>
      <div className="grid gap-2">
        <Label>{t("fields.channel")}</Label>
        <Select
          value={channel}
          disabled={editing || !catalogState.authoritative || catalog.length === 0}
          onValueChange={(value) => {
            const meta = channelMeta(value, catalog);
            setChannel(value);
            setSettings(initSettingsState(provider?.settings_json, meta));
            setSettingsMetaResolved(meta !== undefined);
          }}
        >
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            {(["api_key", "oauth_tokens", "service_account", "github_token"] as const).map((family) => {
              const group = catalog.filter((c) => c.family === family);
              if (group.length === 0) return null;
              return (
                <SelectGroup key={family}>
                  <SelectLabel>{t(`family.${family}`)}</SelectLabel>
                  {group.map((c) => (
                    <SelectItem key={c.id} value={c.id}>
                      <span>{c.displayName}</span>
                      {c.displayName !== c.id && (
                        <span className="font-mono text-xs text-muted-foreground">{c.id}</span>
                      )}
                    </SelectItem>
                  ))}
                </SelectGroup>
              );
            })}
          </SelectContent>
        </Select>
        {editing && <p className="text-xs text-muted-foreground">{t("form.channelLocked")}</p>}
        {!selectedMeta && (
          <p className="text-xs text-destructive">{t("catalog.metadataUnavailable")}</p>
        )}
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
        <ProxyConnectivityTest scope="provider" proxyUrl={proxyUrl} />
      </div>

      <SettingsFields
        channel={channel}
        meta={selectedMeta}
        state={settings}
        onChange={(next) => setSettings((prev) => ({ ...prev, ...next }))}
      />

      <TlsFingerprintField value={tls} onChange={setTls} label={t("fields.tlsProfile")} />

      <div className="flex items-center justify-between">
        <Label htmlFor="p-enabled">{t("fields.enabled")}</Label>
        <Switch id="p-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>
      {formError && <p className="text-sm text-destructive">{formError}</p>}
      <Button
        type="submit"
        disabled={
          mutation.isPending
          || !selectedMeta
          || !catalogState.authoritative
        }
      >
        {editing ? t("form.edit") : t("form.create")}
      </Button>
    </form>
  );
}
