import { ChevronDownIcon } from "lucide-react"
import type { PortalModelDto } from "@/generated/PortalModelDto"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"

export function ModelCapabilities({ model }: { model: PortalModelDto }) {
  const summary = [...new Set(model.capabilities.map((item) => item.group))].join(" · ")
  return (
    <Collapsible className="min-w-0">
      <CollapsibleTrigger asChild>
        <Button type="button" variant="ghost" size="sm" className="group w-full min-w-0 justify-start" aria-label={`${model.name}: ${summary}`}>
          <span className="min-w-0 truncate font-mono text-xs">{summary}</span>
          <Badge variant="secondary">{model.capabilities.length}</Badge>
          <ChevronDownIcon className="group-data-[state=open]:rotate-180" />
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <ul className="flex min-w-0 flex-col gap-2 pt-2">
          {model.capabilities.map((capability) => (
            <li key={`${capability.source}:${capability.operation}:${capability.group}`} className="flex flex-wrap items-center gap-2">
              <Badge variant="outline"><code>{capability.group}</code></Badge>
              <code className="break-all text-xs">{capability.source}</code>
              <code className="break-all text-xs text-muted-foreground">{capability.operation}</code>
            </li>
          ))}
        </ul>
      </CollapsibleContent>
    </Collapsible>
  )
}
