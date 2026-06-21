import { useTranslation } from "react-i18next";
import { channelMeta } from "@/lib/channel-meta";
import { JsonField, parseJsonText } from "@/components/form/json-field";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

interface SecretEditorProps {
  channel: string;
  /** Raw editor text. api_key family: bare key string. Others: JSON text. */
  value: string;
  onChange: (text: string) => void;
  editing: boolean;
}

/** Returns the plaintext secret_json for submission, or null when invalid/empty. */
export function buildSecret(channel: string, text: string): unknown | null {
  const meta = channelMeta(channel);
  const trimmed = text.trim();
  if (trimmed === "") return null;
  // Unknown channels render the api_key input — keep serialization consistent.
  if (!meta || meta.family === "api_key") return { api_key: trimmed };
  const parsed = parseJsonText(trimmed);
  if (!parsed.ok) return null;
  if (meta.family === "service_account") {
    const v = parsed.value as Record<string, unknown>;
    if (typeof v?.client_email !== "string" || v.client_email.trim() === "") return null;
    if (typeof v?.private_key !== "string" || v.private_key.trim() === "") return null;
  }
  return parsed.value;
}

export function secretTemplateText(channel: string): string {
  const meta = channelMeta(channel);
  if (!meta || meta.family === "api_key") return "";
  return JSON.stringify(meta.secretTemplate, null, 2);
}

export function SecretEditor({ channel, value, onChange, editing }: SecretEditorProps) {
  const { t } = useTranslation("providers");
  const meta = channelMeta(channel);
  const family = meta?.family ?? "api_key";

  const label =
    family === "api_key" ? t("secret.apiKey")
    : family === "service_account" ? t("secret.saJson")
    : family === "github_token" ? t("secret.githubToken")
    : t("secret.tokensJson");

  const hint = [
    editing ? t("creds.secretKept") : null,
    family === "service_account" ? t("secret.saInvalid") : null,
    family === "oauth_tokens" ? t("secret.tokensHint") : null,
    meta?.hintKey ? t(`secret.${meta.hintKey}`) : null,
  ].filter(Boolean).join(" ");

  if (family === "api_key") {
    return (
      <div className="grid gap-2">
        <Label htmlFor="c-secret">{label}</Label>
        <Input
          id="c-secret"
          type="password"
          autoComplete="off"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={editing ? "••••••••" : "sk-…"}
        />
        {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
      </div>
    );
  }
  return (
    <div className="grid gap-2">
      <Label htmlFor="c-secret">{label}</Label>
      <JsonField id="c-secret" value={value} onChange={onChange} rows={8} hint={hint} placeholder={secretTemplateText(channel)} />
    </div>
  );
}
