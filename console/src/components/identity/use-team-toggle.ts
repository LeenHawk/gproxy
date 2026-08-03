import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { teamsQuery, upsertTeam, type Team } from "@/api/identity";

/** Team writes are full POST upserts; preserve identity fields when toggling enabled. */
export function useTeamToggle(orgId: number) {
  const queryClient = useQueryClient();
  const queryKey = teamsQuery(orgId).queryKey;

  return useMutation({
    mutationFn: ({ team, enabled }: { team: Team; enabled: boolean }) => upsertTeam(orgId, {
      id: team.id,
      org_id: team.org_id,
      name: team.name,
      enabled,
    }),
    onMutate: async ({ team, enabled }) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<Team[]>(queryKey);
      queryClient.setQueryData<Team[]>(queryKey, (current) =>
        current?.map((item) => (item.id === team.id ? { ...item, enabled } : item)),
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
