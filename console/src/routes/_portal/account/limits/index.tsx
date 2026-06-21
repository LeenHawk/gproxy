import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { myQuotaQuery, myRateLimitsQuery, myPermissionsQuery } from "@/api/portal";
import {
  EffectiveQuotaSection,
  EffectiveRateLimitsSection,
  EffectivePermissionsSection,
} from "@/components/portal/effective-rules";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export const Route = createFileRoute("/_portal/account/limits/")({
  component: LimitsPage,
});

function LimitsPage() {
  const { t } = useTranslation("portal");

  const { data: quotaData, isPending: quotaPending } = useQuery(myQuotaQuery);
  const { data: rateLimitsData, isPending: rateLimitsPending } = useQuery(myRateLimitsQuery);
  const { data: permissionsData, isPending: permissionsPending } = useQuery(myPermissionsQuery);

  return (
    <div className="grid gap-6 p-4 md:p-6">
      <header>
        <h1 className="font-heading text-2xl font-semibold">{t("pages.limits.title")}</h1>
        <p className="text-sm text-muted-foreground">{t("pages.limits.subtitle")}</p>
      </header>

      {/* Quota */}
      <Card>
        <CardHeader>
          <CardTitle>{t("limits.quota")}</CardTitle>
        </CardHeader>
        <CardContent>
          <EffectiveQuotaSection data={quotaData} isPending={quotaPending} />
        </CardContent>
      </Card>

      {/* Rate Limits */}
      <Card>
        <CardHeader>
          <CardTitle>{t("limits.rateLimits")}</CardTitle>
        </CardHeader>
        <CardContent>
          <EffectiveRateLimitsSection data={rateLimitsData} isPending={rateLimitsPending} />
        </CardContent>
      </Card>

      {/* Route Permissions */}
      <Card>
        <CardHeader>
          <CardTitle>{t("limits.permissions")}</CardTitle>
        </CardHeader>
        <CardContent>
          <EffectivePermissionsSection data={permissionsData} isPending={permissionsPending} />
        </CardContent>
      </Card>
    </div>
  );
}
