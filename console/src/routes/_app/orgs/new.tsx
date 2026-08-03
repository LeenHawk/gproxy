import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { OrgForm } from "@/components/identity/org-form";

export const Route = createFileRoute("/_app/orgs/new")({
  component: NewOrgPage,
});

function NewOrgPage() {
  const { t } = useTranslation("identity");
  const navigate = useNavigate();

  return (
    <div className="grid gap-5 p-4 md:p-6">
      <h2 className="text-xl font-semibold">{t("orgs.new")}</h2>
      <div className="max-w-xl rounded-lg border p-4">
        <OrgForm
          onSaved={(org) => {
            void navigate({
              to: "/orgs/$orgId",
              params: { orgId: String(org.id) },
              search: { tab: "profile" },
            });
          }}
        />
      </div>
    </div>
  );
}
