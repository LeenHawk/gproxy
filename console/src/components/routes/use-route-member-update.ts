import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { routeMembersQuery, upsertRouteMember, type RouteMember } from "@/api/routes";

export type MemberChanges = Partial<Pick<RouteMember, "enabled" | "tier" | "weight">>;

/** Inline member writes remain full-entity upserts, with optimistic rollback. */
export function useRouteMemberUpdate(routeId: number) {
  const queryClient = useQueryClient();
  const queryKey = routeMembersQuery(routeId).queryKey;

  return useMutation({
    mutationFn: ({ member, changes }: { member: RouteMember; changes: MemberChanges }) => {
      const updated = { ...member, ...changes };
      return upsertRouteMember(routeId, {
        id: updated.id,
        route_id: updated.route_id,
        provider_id: updated.provider_id,
        upstream_model_id: updated.upstream_model_id,
        weight: updated.weight,
        tier: updated.tier,
        enabled: updated.enabled,
      });
    },
    onMutate: async ({ member, changes }) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<RouteMember[]>(queryKey);
      queryClient.setQueryData<RouteMember[]>(queryKey, (current) =>
        current?.map((item) => (item.id === member.id ? { ...item, ...changes } : item)),
      );
      return { previous };
    },
    onError: (error, _variables, context) => {
      if (context?.previous) queryClient.setQueryData(queryKey, context.previous);
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey }),
  });
}
