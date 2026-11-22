import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { MCP_NATURES, mcpToolNatures, mcpTools } from "@/api/mcp"
import type { McpToolDto } from "@/generated/app"
import { MaybeCell } from "@/components/cells"
import { DataTable } from "@/components/data-table"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Badge } from "@/components/ui/badge"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

const DECLARED = "__declared"

/** The tools of an MCP server, and the nature an administrator gives them over the server's own word. */
export function ProviderTools({ providerId }: { providerId: string }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const tools = useQuery({ queryKey: ["mcp-tools", providerId], queryFn: () => mcpTools(providerId) })
  const override = useMutation({
    mutationFn: async ({ tool, nature }: { tool: McpToolDto; nature: string }) => {
      if (nature === DECLARED) await mcpToolNatures.remove(tool.overrideId!)
      else if (tool.overrideId) await mcpToolNatures.update(tool.overrideId, { nature })
      else await mcpToolNatures.create({ providerId, tool: tool.name, nature })
    },
    onSuccess: () => client.invalidateQueries({ queryKey: ["mcp-tools", providerId] }),
  })
  const nature = (value: string) => t(`mcp.natures.${value}`, { defaultValue: value })
  return (
    <div className="flex flex-col gap-4">
      {override.error ? <ErrorNotice error={override.error} /> : null}
      <QueryState isPending={tools.isPending} error={tools.error}>
        <DataTable
          columns={[
            { key: "name", cell: (row) => <span className="font-mono text-xs">{row.name}</span> },
            { key: "description", header: t("form.description"), cell: (row) => <span title={row.description ?? undefined}><MaybeCell value={row.description} /></span> },
            { key: "declaredNature", header: t("mcp.declaredNature"), cell: (row) => nature(row.declaredNature) },
            { key: "nature", header: t("mcp.nature"), cell: (row) => <Badge variant={row.nature === "destructive" ? "destructive" : row.nature === "write" ? "warning" : "outline"}>{nature(row.nature)}</Badge> },
            {
              key: "override",
              header: t("mcp.override"),
              cell: (row) => (
                <Select value={row.overrideId ? row.nature : DECLARED} disabled={override.isPending} onValueChange={(next) => override.mutate({ tool: row, nature: next })}>
                  <SelectTrigger size="sm" aria-label={`${t("mcp.override")}: ${row.name}`}><SelectValue /></SelectTrigger>
                  <SelectContent><SelectGroup>
                    <SelectItem value={DECLARED}>{t("mcp.declared")}</SelectItem>
                    {MCP_NATURES.map((value) => <SelectItem key={value} value={value}>{nature(value)}</SelectItem>)}
                  </SelectGroup></SelectContent>
                </Select>
              ),
            },
          ]}
          rows={tools.data ?? []}
          rowKey={(row) => row.name}
          empty={<EmptyNotice title={t("state.emptyTitle")} />}
        />
      </QueryState>
    </div>
  )
}
