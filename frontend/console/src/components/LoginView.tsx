import { useState } from "react";

import { useI18n } from "../app/i18n";
import { Button, Card, Input, Label } from "./ui";

export function LoginView({
  onLogin,
  loading,
}: {
  onLogin: (name: string, password: string) => Promise<void>;
  loading: boolean;
}) {
  const { t } = useI18n();
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");

  return (
    <div className="mx-auto mt-20 w-full max-w-lg px-4">
      <Card title={t("login.title")} subtitle={t("login.subtitle")}>
        <form
          className="space-y-3"
          onSubmit={async (event) => {
            event.preventDefault();
            setError("");
            try {
              await onLogin(name.trim(), password);
            } catch (err) {
              setError(err instanceof Error ? err.message : String(err));
            }
          }}
        >
          <div>
            <Label>{t("login.username")}</Label>
            <Input
              value={name}
              onChange={setName}
              placeholder={t("login.usernamePlaceholder")}
            />
          </div>
          <div>
            <Label>{t("login.password")}</Label>
            <Input
              type="password"
              value={password}
              onChange={setPassword}
              placeholder={t("login.passwordPlaceholder")}
            />
          </div>
          {error ? <p className="text-sm text-red-500">{error}</p> : null}
          <Button type="submit" disabled={loading}>
            {loading ? t("login.submitting") : t("login.submit")}
          </Button>
        </form>
      </Card>
    </div>
  );
}
