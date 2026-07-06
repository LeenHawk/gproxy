import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { providerQuery } from "@/api/providers";
import { ModelsTab } from "@/components/providers/models-tab";

export const Route = createFileRoute("/_app/providers/$providerId/models")({
  component: ModelsSection,
});

function ModelsSection() {
  const { providerId } = Route.useParams();
  const { data: provider } = useSuspenseQuery(providerQuery(Number(providerId)));
  return <ModelsTab provider={provider} />;
}
