import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { portalSessionQuery } from "@/api/portal";
import { ChangePasswordForm } from "@/components/portal/change-password-form";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export const Route = createFileRoute("/_portal/account/security/")({
  component: SecurityPage,
});

function SecurityPage() {
  const { t } = useTranslation("portal");
  const { data: user } = useQuery(portalSessionQuery);

  return (
    <div className="grid gap-6 p-4 md:p-6">
      <header>
        <h1 className="font-heading text-2xl font-semibold">{t("pages.account.title")}</h1>
        <p className="text-sm text-muted-foreground">{t("pages.account.subtitle")}</p>
      </header>

      <ChangePasswordForm />

      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t("account.identity.title")}</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-2 text-sm">
          <div className="flex items-center gap-4">
            <span className="w-24 font-medium text-muted-foreground">{t("account.identity.name")}</span>
            <span>{user?.name ?? "—"}</span>
          </div>
          <div className="flex items-center gap-4">
            <span className="w-24 font-medium text-muted-foreground">{t("account.identity.org")}</span>
            <span>{user?.org_name ?? "—"}</span>
          </div>
          {user?.team_id != null && (
            <div className="flex items-center gap-4">
              <span className="w-24 font-medium text-muted-foreground">{t("account.identity.team")}</span>
              <span>{user.team_name ?? "—"}</span>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
