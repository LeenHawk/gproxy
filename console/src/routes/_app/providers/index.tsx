import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/providers/")({
  component: ProvidersIndex,
});

function ProvidersIndex() {
  return null;
}
