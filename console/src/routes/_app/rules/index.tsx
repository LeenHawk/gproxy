import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/rules/")({
  component: RulesIndex,
});

function RulesIndex() {
  return null;
}
