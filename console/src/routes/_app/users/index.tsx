import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/users/")({
  component: UsersIndex,
});

function UsersIndex() {
  return null;
}
