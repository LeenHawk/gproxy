import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { upsertUserKey, userKeysQuery, type UserKeyView } from "@/api/identity";

/** User-key writes are full POST upserts; preserve the label when changing enabled. */
export function useUserKeyToggle(userId: number) {
  const queryClient = useQueryClient();
  const queryKey = userKeysQuery(userId).queryKey;

  return useMutation({
    mutationFn: ({ key, enabled }: { key: UserKeyView; enabled: boolean }) =>
      upsertUserKey(userId, { id: key.id, label: key.label, enabled }),
    onMutate: async ({ key, enabled }) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<UserKeyView[]>(queryKey);
      queryClient.setQueryData<UserKeyView[]>(queryKey, (current) =>
        current?.map((item) => (item.id === key.id ? { ...item, enabled } : item)),
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
