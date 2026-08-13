import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { updateStatusQuery } from "@/api/update";
import { UpdateChannelCard } from "@/components/update/update-channel-card";
import { UpdatePanel } from "@/components/update/update-panel";

export const Route = createFileRoute("/_app/update/")({
  loader: ({ context }) => {
    void context.queryClient.ensureQueryData(updateStatusQuery);
  },
  component: UpdatePage,
});

function UpdatePage() {
  const { t } = useTranslation("update");
  const { data: status } = useQuery(updateStatusQuery);
  return (
    <div className="grid gap-4 p-4 md:p-6">
      <div>
        <h1 className="text-xl font-semibold">{t("title")}</h1>
        <p className="text-sm text-muted-foreground">{t("subtitle")}</p>
      </div>
      {status !== undefined && status.state !== "unavailable" && <UpdateChannelCard />}
      <UpdatePanel />
    </div>
  );
}
