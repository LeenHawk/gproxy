import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { RuleSetForm } from "@/components/rules/rule-set-form";

export const Route = createFileRoute("/_app/rules/new")({
  component: NewRuleSetPage,
});

function NewRuleSetPage() {
  const { t } = useTranslation("rules");
  const navigate = useNavigate();

  return (
    <div className="grid gap-5 p-4 md:p-6">
      <h2 className="text-xl font-semibold">{t("ruleSet.add")}</h2>
      <div className="max-w-xl rounded-lg border p-4">
        <RuleSetForm
          onSaved={(ruleSet) => {
            void navigate({ to: "/rules/$ruleSetId", params: { ruleSetId: String(ruleSet.id) } });
          }}
        />
      </div>
    </div>
  );
}
