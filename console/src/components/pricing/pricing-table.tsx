import { Pencil, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { PriceRule } from "@/api/price-rules";
import { DataTable, type DataColumn, type DataTableSelection } from "@/components/data-table";
import { EnabledToggle } from "@/components/enabled-toggle";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { usePriceRuleToggle } from "@/hooks/use-price-rule-toggle";

interface PricingTableProps {
  rows: PriceRule[];
  providerNames: Map<number, string>;
  batchMode: boolean;
  selection?: DataTableSelection;
  onEdit: (rule: PriceRule) => void;
  onDelete: (id: number) => void;
}

export function PricingTable({
  rows,
  providerNames,
  batchMode,
  selection,
  onEdit,
  onDelete,
}: PricingTableProps) {
  const { t } = useTranslation("pricing");
  const toggle = usePriceRuleToggle();
  const priceCell = (value: string) => <span className="font-mono text-xs tabular-nums">{value}</span>;
  const columns: DataColumn<PriceRule>[] = [
    {
      key: "scope",
      label: t("columns.scope"),
      header: t("columns.scope"),
      cell: (rule) => (
        <Badge variant={rule.provider_id == null ? "outline" : "secondary"}>
          {rule.provider_id == null ? t("scope.global") : providerNames.get(rule.provider_id) ?? `#${rule.provider_id}`}
        </Badge>
      ),
    },
    {
      key: "match",
      label: t("columns.match"),
      header: t("columns.match"),
      cell: (rule) => (
        <div className="flex items-center gap-2">
          <span className="font-mono text-sm">{rule.model_match}</span>
          <Badge variant="outline" className="text-[10px]">{t(`match.${rule.match_type}`)}</Badge>
        </div>
      ),
    },
    { key: "input_price", label: t("columns.inputPrice"), header: t("columns.inputPrice"), cell: (rule) => priceCell(rule.input_price) },
    { key: "output_price", label: t("columns.outputPrice"), header: t("columns.outputPrice"), cell: (rule) => priceCell(rule.output_price) },
    { key: "cache_read_price", label: t("columns.cacheReadPrice"), header: t("columns.cacheReadPrice"), cell: (rule) => priceCell(rule.cache_read_price) },
    { key: "cache_creation_5m_price", label: t("columns.cacheCreation5mPrice"), header: t("columns.cacheCreation5mPrice"), cell: (rule) => priceCell(rule.cache_creation_5m_price) },
    { key: "cache_creation_30m_price", label: t("columns.cacheCreation30mPrice"), header: t("columns.cacheCreation30mPrice"), cell: (rule) => priceCell(rule.cache_creation_30m_price) },
    { key: "cache_creation_1h_price", label: t("columns.cacheCreation1hPrice"), header: t("columns.cacheCreation1hPrice"), cell: (rule) => priceCell(rule.cache_creation_1h_price) },
    { key: "image_output_price", label: t("columns.imageOutputPrice"), header: t("columns.imageOutputPrice"), cell: (rule) => priceCell(rule.image_output_price) },
    {
      key: "enabled",
      label: t("columns.status"),
      header: t("columns.status"),
      cell: (rule) => (
        <EnabledToggle
          enabled={rule.enabled}
          pending={toggle.isPending}
          onToggle={(enabled) => toggle.mutate({ rule, enabled })}
        />
      ),
    },
    ...(batchMode ? [] : [{
      key: "actions",
      label: t("columns.actions"),
      header: "",
      cell: (rule) => (
        <div className="flex justify-end gap-1">
          <Button size="icon" variant="ghost" onClick={(event) => { event.stopPropagation(); onEdit(rule); }} aria-label={t("actions.edit")}>
            <Pencil className="size-4" aria-hidden />
          </Button>
          <Button size="icon" variant="ghost" onClick={(event) => { event.stopPropagation(); onDelete(rule.id); }} aria-label={t("actions.delete")}>
            <Trash2 className="size-4" aria-hidden />
          </Button>
        </div>
      ),
      className: "w-24 text-right",
    } as DataColumn<PriceRule>]),
  ];

  return (
    <DataTable
      columns={columns}
      rows={rows}
      rowKey={(rule) => rule.id}
      empty={t("empty")}
      columnToggle={{ storageKey: "gproxy.pricing.columns", label: t("columns.toggle") }}
      onRowClick={batchMode ? undefined : onEdit}
      selection={selection}
      renderCard={(rule) => (
        <div className="grid gap-2">
          <div className="flex items-center justify-between gap-2">
            <div className="flex min-w-0 items-center gap-2">
              <span className="truncate font-mono text-sm">{rule.model_match}</span>
              <Badge variant="outline" className="shrink-0 text-[10px]">{t(`match.${rule.match_type}`)}</Badge>
            </div>
            <EnabledToggle
              enabled={rule.enabled}
              pending={toggle.isPending}
              onToggle={(enabled) => toggle.mutate({ rule, enabled })}
            />
          </div>
          <div className="text-xs text-muted-foreground">
            {rule.provider_id == null ? t("scope.global") : providerNames.get(rule.provider_id) ?? `#${rule.provider_id}`}
          </div>
          <div className="grid grid-cols-2 gap-x-3 gap-y-1 font-mono text-xs text-muted-foreground">
            <span>{t("columns.inputPrice")}: {rule.input_price}</span>
            <span>{t("columns.outputPrice")}: {rule.output_price}</span>
            <span>{t("columns.cacheReadPrice")}: {rule.cache_read_price}</span>
            <span>{t("columns.cacheCreation5mPrice")}: {rule.cache_creation_5m_price}</span>
            <span>{t("columns.cacheCreation30mPrice")}: {rule.cache_creation_30m_price}</span>
            <span>{t("columns.cacheCreation1hPrice")}: {rule.cache_creation_1h_price}</span>
            <span>{t("columns.imageOutputPrice")}: {rule.image_output_price}</span>
          </div>
        </div>
      )}
    />
  );
}
