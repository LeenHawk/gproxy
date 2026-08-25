import { useTranslation } from "react-i18next"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { QuotaWindowDto } from "@/generated/QuotaWindowDto"
import type { UsageAggregateDto } from "@/generated/UsageAggregateDto"
import type { UsageGroupByDto } from "@/generated/UsageGroupByDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Field, FieldLabel } from "@/components/ui/field"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { UsageTable } from "@/components/usage/usage-table"
import { WindowList } from "@/components/usage/window-list"

const groups: Array<UsageGroupByDto> = ["user_key", "user", "provider", "model"]

export function UsageExplorer({ group, onGroup, rangeDays, onRangeDays, rows, quotas, cycles, providers, users, keys }: { group: UsageGroupByDto; onGroup: (group: UsageGroupByDto) => void; rangeDays: number; onRangeDays: (days: number) => void; rows: Array<UsageAggregateDto>; quotas: Array<QuotaWindowDto>; cycles: Array<CredentialQuotaCycleDto>; providers: Array<ProviderDto>; users: Array<UserDto>; keys: Array<UserKeyDto> }) {
  const { t } = useTranslation()
  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <ToggleGroup type="single" variant="outline" className="flex-wrap justify-start" value={group} onValueChange={(value) => value && onGroup(value as UsageGroupByDto)} aria-label={t("usage.groupBy.label")}>
          {groups.map((value) => <ToggleGroupItem key={value} value={value}>{t(`usage.groupBy.${value}`)}</ToggleGroupItem>)}
        </ToggleGroup>
        <Field className="w-auto">
          <FieldLabel htmlFor="usage-range">{t("usage.range.label")}</FieldLabel>
          <Select value={String(rangeDays)} onValueChange={(value) => onRangeDays(Number(value))}>
            <SelectTrigger id="usage-range"><SelectValue /></SelectTrigger>
            <SelectContent><SelectGroup>{[1, 7, 30].map((days) => <SelectItem key={days} value={String(days)}>{t(`usage.range.${days}`)}</SelectItem>)}</SelectGroup></SelectContent>
          </Select>
        </Field>
      </div>
      <Tabs defaultValue="cost">
        <TabsList><TabsTrigger value="cost">{t("usage.cost.title")}</TabsTrigger><TabsTrigger value="windows">{t("usage.windows")}</TabsTrigger></TabsList>
        <TabsContent value="cost" className="pt-4"><UsageTable rows={rows} group={group} providers={providers} users={users} keys={keys} /></TabsContent>
        <TabsContent value="windows" className="pt-5"><WindowList quotas={quotas} cycles={cycles} users={users} keys={keys} /></TabsContent>
      </Tabs>
    </div>
  )
}
