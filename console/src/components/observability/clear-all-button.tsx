import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { clearObservabilityData, type ObservabilityData } from "@/api/usage";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { Button } from "@/components/ui/button";

const queryKeys: Record<ObservabilityData, string[]> = {
  usage: ["usage", "usage-rollups"],
  logs: ["logs"],
  audit: ["audit"],
};

export function ClearAllButton({
  data,
  onCleared,
}: {
  data: ObservabilityData;
  onCleared?: () => void;
}) {
  const { t } = useTranslation("observability");
  const queryClient = useQueryClient();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const mutation = useMutation({
    mutationFn: () => clearObservabilityData(data),
    onSuccess: () => {
      for (const queryKey of queryKeys[data]) {
        void queryClient.invalidateQueries({ queryKey: [queryKey] });
      }
      onCleared?.();
      setConfirmOpen(false);
      toast.success(t(`cleanup.${data}.success`));
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
  });

  return (
    <>
      <Button
        variant="outline"
        size="sm"
        className="text-destructive"
        disabled={mutation.isPending}
        onClick={() => setConfirmOpen(true)}
      >
        <Trash2 className="size-4" aria-hidden />
        {t("cleanup.deleteAll")}
      </Button>
      <ConfirmDangerous
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t(`cleanup.${data}.title`)}
        description={t(`cleanup.${data}.description`)}
        confirmLabel={t("cleanup.confirm")}
        onConfirm={() => mutation.mutate()}
        pending={mutation.isPending}
      />
    </>
  );
}
