import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/routes/")({
  component: RoutesIndex,
});

function RoutesIndex() {
  return null;
}
