import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { ProviderForm } from "@/components/providers/provider-form";

export const Route = createFileRoute("/_app/providers/new")({
  component: NewProviderPage,
});

function NewProviderPage() {
  const { t } = useTranslation("providers");
  const navigate = useNavigate();

  return (
    <div className="grid gap-5 p-4 md:p-6">
      <h2 className="text-xl font-semibold">{t("new")}</h2>
      <div className="max-w-2xl rounded-lg border p-4">
        <ProviderForm
          onSaved={(provider) => {
            void navigate({
              to: "/providers/$providerId/credentials",
              params: { providerId: String(provider.id) },
            });
          }}
        />
      </div>
    </div>
  );
}
