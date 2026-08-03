import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/orgs/")({
  component: OrgsIndex,
});

function OrgsIndex() {
  return null;
}
