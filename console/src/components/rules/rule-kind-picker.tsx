import { DatabaseIcon, HeadingIcon, PencilIcon, TypeIcon, WandSparklesIcon, type LucideIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { RuleConfigDto } from "@/generated/RuleConfigDto"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"

const RULE_KINDS: Array<RuleConfigDto["kind"]> = ["system_text", "cache_breakpoint", "rewrite", "transform", "header"]

const ICONS: Record<RuleConfigDto["kind"], LucideIcon> = {
  system_text: TypeIcon,
  cache_breakpoint: DatabaseIcon,
  rewrite: PencilIcon,
  transform: WandSparklesIcon,
  header: HeadingIcon,
}

export function RuleKindPicker({ value, onChange }: { value: RuleConfigDto["kind"]; onChange: (value: RuleConfigDto["kind"]) => void }) {
  const { t } = useTranslation()
  return <ToggleGroup
    type="single"
    value={value}
    variant="outline"
    className="grid w-full grid-cols-1 items-stretch sm:grid-cols-2"
    aria-label={t("rules.fields.kind")}
    onValueChange={(next) => { if (next) onChange(next as RuleConfigDto["kind"]) }}
  >
    {RULE_KINDS.map((kind) => {
      const Icon = ICONS[kind]
      return <ToggleGroupItem key={kind} value={kind} className="h-auto min-w-0 items-start justify-start gap-3 whitespace-normal p-3 text-left">
        <Icon className="mt-0.5 shrink-0" aria-hidden />
        <span className="flex min-w-0 flex-col gap-0.5">
          <span>{t(`rules.kinds.${kind}`)}</span>
          <span className="text-xs font-normal text-muted-foreground">{t(`rules.kindDesc.${kind}`)}</span>
        </span>
      </ToggleGroupItem>
    })}
  </ToggleGroup>
}
