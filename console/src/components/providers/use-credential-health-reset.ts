import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { clearCredentialModelStatuses, clearCredentialStatus } from "@/api/credentials";

/** Both the per-credential badges and the provider-list roll-up read these. */
function invalidateHealth(
  queryClient: ReturnType<typeof useQueryClient>,
  credentialId: number,
): void {
  for (const queryKey of [
    ["credentials", credentialId, "status"],
    ["credentials", credentialId, "model-statuses"],
    ["credential-statuses"],
    ["credential-model-statuses"],
  ]) {
    void queryClient.invalidateQueries({ queryKey });
  }
}

function reportError(error: unknown): void {
  toast.error(error instanceof ApiError ? error.message : String(error));
}

/**
 * Operator reset of a credential's health. Clearing only resets the decision
 * inputs — a still-failing upstream re-trips the breaker on the next attempt,
 * and other instances keep their own soft state.
 */
export function useCredentialHealthReset(credentialId: number, onDone?: () => void) {
  const queryClient = useQueryClient();

  const credential = useMutation({
    mutationFn: () => clearCredentialStatus(credentialId),
    onSuccess: () => {
      invalidateHealth(queryClient, credentialId);
      onDone?.();
    },
    onError: reportError,
  });

  const models = useMutation({
    mutationFn: () => clearCredentialModelStatuses(credentialId),
    onSuccess: () => {
      invalidateHealth(queryClient, credentialId);
      onDone?.();
    },
    onError: reportError,
  });

  return { credential, models };
}
