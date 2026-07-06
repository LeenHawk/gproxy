import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { providerQuery } from "@/api/providers";
import { CredentialsTab } from "@/components/providers/credentials-tab";

export const Route = createFileRoute("/_app/providers/$providerId/credentials")({
  component: CredentialsSection,
});

function CredentialsSection() {
  const { providerId } = Route.useParams();
  const { data: provider } = useSuspenseQuery(providerQuery(Number(providerId)));
  return <CredentialsTab provider={provider} />;
}
