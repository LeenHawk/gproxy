import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/providers/$providerId/")({
  beforeLoad: ({ params }) => {
    // 凭证最优先:进入详情默认落到凭证分区
    throw redirect({ to: "/providers/$providerId/credentials", params, replace: true });
  },
});
