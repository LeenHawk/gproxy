import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { RouteForm } from "@/components/routes/route-form";

export const Route = createFileRoute("/_app/routes/new")({
  component: NewRoutePage,
});

function NewRoutePage() {
  const { t } = useTranslation("routes");
  const navigate = useNavigate();

  return (
    <div className="grid gap-5 p-4 md:p-6">
      <h2 className="text-xl font-semibold">{t("new")}</h2>
      <div className="max-w-2xl rounded-lg border p-4">
        <RouteForm
          onSaved={(route) => {
            void navigate({
              to: "/routes/$routeId",
              params: { routeId: String(route.id) },
              search: { tab: "settings" },
            });
          }}
        />
      </div>
    </div>
  );
}
