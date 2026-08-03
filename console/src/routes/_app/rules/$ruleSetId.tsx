import { useState } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ruleSetQuery, deleteRuleSet } from "@/api/rules";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { RuleSetEditor } from "@/components/rules/rule-set-editor";
import { RuleSetForm } from "@/components/rules/rule-set-form";
import { Button } from "@/components/ui/button";

export const Route = createFileRoute("/_app/rules/$ruleSetId")({
  loader: ({ context, params }) => {
    const id = Number(params.ruleSetId);
    if (Number.isNaN(id)) throw redirect({ to: "/rules" });
    return context.queryClient.ensureQueryData(ruleSetQuery(id));
  },
  component: RuleSetDetailPage,
});

function RuleSetDetailPage() {
  const { ruleSetId } = Route.useParams();
  const id = Number(ruleSetId);
  const { t } = useTranslation("rules");
  const { t: tCommon } = useTranslation("common");
  const navigate = useNavigate();
  const qc = useQueryClient();
  const { data: ruleSet } = useSuspenseQuery(ruleSetQuery(id));

  const [deleteRsOpen, setDeleteRsOpen] = useState(false);

  const rsRemoval = useMutation({
    mutationFn: () => deleteRuleSet(id),
    onSuccess: () => {
      setDeleteRsOpen(false);
      void qc.invalidateQueries({ queryKey: ["rule-sets"] });
      void navigate({ to: "/rules" });
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : String(e)),
  });

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-xl font-semibold">{ruleSet.name}</h1>
        <Button
          variant="ghost"
          size="sm"
          className="text-destructive"
          onClick={() => setDeleteRsOpen(true)}
        >
          <Trash2 className="size-4" />
        </Button>
      </div>

      <div className="max-w-xl rounded-lg border p-4">
        <RuleSetForm key={`${ruleSet.id}-${ruleSet.updated_at}-${ruleSet.enabled}`} ruleSet={ruleSet} />
      </div>

      <RuleSetEditor ruleSetId={id} showMetadata={false} />

      <ConfirmDangerous
        open={deleteRsOpen}
        onOpenChange={setDeleteRsOpen}
        title={ruleSet.name}
        description={t("ruleSet.deleteConfirm")}
        confirmLabel={tCommon("actions.delete")}
        onConfirm={() => rsRemoval.mutate()}
        pending={rsRemoval.isPending}
      />
    </div>
  );
}
