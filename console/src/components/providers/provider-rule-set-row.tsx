import { Pencil, Settings2, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ProviderRuleSet } from "@/api/rules";
import type { DataColumn } from "@/components/data-table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

interface RowActionsProps {
  a: ProviderRuleSet;
  onEdit: (id: number) => void;
  onAttach: (a: ProviderRuleSet) => void;
  onDetach: (a: ProviderRuleSet) => void;
}

function RowActions({ a, onEdit, onAttach, onDetach }: RowActionsProps) {
  const { t } = useTranslation("rules");
  return (
    <div className="flex justify-end gap-1">
      <Button
        variant="ghost"
        size="icon"
        aria-label={t("rule.list")}
        onClick={(e) => { e.stopPropagation(); onEdit(a.rule_set_id); }}
      >
        <Pencil className="size-4" />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        aria-label={t("providerRuleSet.attach")}
        onClick={(e) => { e.stopPropagation(); onAttach(a); }}
      >
        <Settings2 className="size-4" />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        className="text-destructive"
        aria-label={t("providerRuleSet.unattach")}
        onClick={(e) => { e.stopPropagation(); onDetach(a); }}
      >
        <Trash2 className="size-4" />
      </Button>
    </div>
  );
}

export function buildProviderRuleSetColumns(
  t: ReturnType<typeof useTranslation<"rules">>["t"],
  rsName: Map<number, string>,
  onEdit: (id: number) => void,
  onAttach: (a: ProviderRuleSet) => void,
  onDetach: (a: ProviderRuleSet) => void,
): DataColumn<ProviderRuleSet>[] {
  return [
    {
      key: "name",
      header: t("providerRuleSet.ruleSet"),
      cell: (a) => (
        <span className="font-medium">{rsName.get(a.rule_set_id) ?? `#${a.rule_set_id}`}</span>
      ),
    },
    {
      key: "sort",
      header: t("providerRuleSet.sortOrder"),
      cell: (a) => a.sort_order,
      className: "w-16",
    },
    {
      key: "enabled",
      header: t("providerRuleSet.enabled"),
      cell: (a) => (
        <Badge variant={a.enabled ? "secondary" : "outline"}>{a.enabled ? "on" : "off"}</Badge>
      ),
    },
    {
      key: "actions",
      header: "",
      className: "w-28 text-right",
      cell: (a) => (
        <RowActions a={a} onEdit={onEdit} onAttach={onAttach} onDetach={onDetach} />
      ),
    },
  ];
}

export function ProviderRuleSetCard({
  a,
  rsName,
  onEdit,
  onAttach,
  onDetach,
}: {
  a: ProviderRuleSet;
  rsName: Map<number, string>;
  onEdit: (id: number) => void;
  onAttach: (a: ProviderRuleSet) => void;
  onDetach: (a: ProviderRuleSet) => void;
}) {
  return (
    <div className="grid gap-2">
      <div className="flex items-center justify-between">
        <span className="font-medium">{rsName.get(a.rule_set_id) ?? `#${a.rule_set_id}`}</span>
        <Badge variant={a.enabled ? "secondary" : "outline"}>{a.enabled ? "on" : "off"}</Badge>
      </div>
      <div className="flex items-center justify-between">
        <span className="text-xs text-muted-foreground">#{a.sort_order}</span>
        <RowActions a={a} onEdit={onEdit} onAttach={onAttach} onDetach={onDetach} />
      </div>
    </div>
  );
}
