import { useEffect, useState } from "react";

import { useI18n } from "../../../app/i18n";
import { apiJson } from "../../../lib/api";
import { authHeaders } from "../../../lib/auth";
import type {
  OAuthCallbackResponse,
  OAuthStartResponse,
  ProviderRow,
} from "../../../lib/types/admin";
import { OAuthTab } from "./OAuthTab";
import { buildOAuthCallbackQuery } from "./oauth";

/// OAuth tab container. Owns the start/callback flow state and handlers.
/// On successful finish it invokes `onFinished` so the parent can reload
/// provider-scoped data and switch to the credentials tab.
export function OAuthPane({
  selectedProvider,
  sessionToken,
  notify,
  onFinished,
}: {
  selectedProvider: ProviderRow | null;
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
  onFinished: () => void;
}) {
  const { t } = useI18n();

  const [oauthFlow, setOauthFlow] = useState<OAuthStartResponse | null>(null);
  const [oauthCallbackUrl, setOauthCallbackUrl] = useState("");
  const [oauthCallbackResult, setOauthCallbackResult] = useState<OAuthCallbackResponse | null>(
    null,
  );

  useEffect(() => {
    setOauthFlow(null);
    setOauthCallbackUrl("");
    setOauthCallbackResult(null);
  }, [selectedProvider?.id]);

  const loadOAuthStart = async () => {
    if (!selectedProvider) {
      notify("error", t("providers.error.needProvider"));
      return;
    }
    try {
      const result = await apiJson<OAuthStartResponse>(
        `/${encodeURIComponent(selectedProvider.name)}/v1/oauth`,
        { headers: authHeaders(sessionToken, false) },
      );
      setOauthFlow(result);
      notify("info", t("providers.oauth.started"));
      window.open(result.authorize_url, "_blank", "noopener,noreferrer");
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const loadOAuthFinish = async () => {
    if (!selectedProvider) {
      notify("error", t("providers.error.needProvider"));
      return;
    }
    try {
      const query = buildOAuthCallbackQuery(oauthCallbackUrl);
      const result = await apiJson<OAuthCallbackResponse>(
        `/${encodeURIComponent(selectedProvider.name)}/v1/oauth/callback${query}`,
        { headers: authHeaders(sessionToken, false) },
      );
      setOauthCallbackResult(result);
      notify("info", t("providers.oauth.finished"));
      onFinished();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <OAuthTab
      flow={oauthFlow}
      callbackUrl={oauthCallbackUrl}
      callbackResult={oauthCallbackResult}
      onChangeCallbackUrl={setOauthCallbackUrl}
      onStart={() => void loadOAuthStart()}
      onOpenAuthorize={() => {
        if (oauthFlow?.authorize_url) {
          window.open(oauthFlow.authorize_url, "_blank", "noopener,noreferrer");
        }
      }}
      onFinish={() => void loadOAuthFinish()}
      labels={{
        start: t("providers.oauth.start"),
        finish: t("providers.oauth.finish"),
        startHint: t("providers.oauth.startHint"),
        openAuthorize: t("providers.oauth.openAuthorize"),
        redirectUri: t("providers.oauth.redirectUri"),
        callbackUrl: t("providers.oauth.callbackUrl"),
        callbackHint: t("providers.oauth.callbackHint"),
        persistedCredential: t("providers.oauth.persistedCredential"),
      }}
    />
  );
}
