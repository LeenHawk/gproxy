import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { credentialsQuery, upsertCredential, type CredentialView } from "@/api/credentials";

/** Omitting secret_json on an update makes the server retain the sealed secret. */
export function useCredentialToggle(providerId: number) {
  const queryClient = useQueryClient();
  const queryKey = credentialsQuery(providerId).queryKey;

  return useMutation({
    mutationFn: ({ credential, enabled }: { credential: CredentialView; enabled: boolean }) =>
      upsertCredential(providerId, {
        id: credential.id,
        label: credential.label,
        kind: credential.kind,
        weight: credential.weight,
        rpm_limit: credential.rpm_limit,
        tpm_limit: credential.tpm_limit,
        proxy_url: credential.proxy_url,
        ...(credential.tls_fingerprint != null
          ? { tls_fingerprint: credential.tls_fingerprint }
          : {}),
        enabled,
      }),
    onMutate: async ({ credential, enabled }) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<CredentialView[]>(queryKey);
      queryClient.setQueryData<CredentialView[]>(queryKey, (current) =>
        current?.map((item) => (item.id === credential.id ? { ...item, enabled } : item)),
      );
      return { previous };
    },
    onError: (error, _variables, context) => {
      if (context) queryClient.setQueryData(queryKey, context.previous);
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey }),
  });
}
