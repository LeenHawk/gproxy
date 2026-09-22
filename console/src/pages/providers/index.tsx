import { useQuery } from "@tanstack/react-query"
import { Waypoints } from "lucide-react"
import { channels, providers, providerPath } from "@/api/configuration"
import { BoolCell, MaybeCell } from "@/components/cells"
import { QueryState } from "@/components/state"
import { CollectionPage } from "@/pages/identity/collection"
import { Link } from "@/lib/router"
import { providerFields } from "@/pages/providers/fields"

export function ProvidersPage() {
  const catalog = useQuery({ queryKey: ["configuration", "channels"], queryFn: channels })
  return (
    <QueryState isPending={catalog.isPending} error={catalog.error}>
      <CollectionPage
        id="providers"
        family={providers}
        searchable
        rowId={(row) => row.id}
        rowLabel={(row) => row.name}
        columns={[
          { key: "name", cell: (row) => (
            <Link to={providerPath(row.id)} className="inline-flex items-center gap-2 font-medium hover:underline">
              <Waypoints className="size-4 shrink-0" aria-hidden="true" />
              {row.name}
            </Link>
          ) },
          { key: "channel", cell: (row) => catalog.data?.find((channel) => channel.id === row.channel)?.displayName ?? row.channel },
          { key: "baseUrl", cell: (row) => <MaybeCell value={row.baseUrl} /> },
          { key: "enabled", cell: (row) => <BoolCell value={row.enabled} /> },
        ]}
        fields={providerFields(catalog.data ?? [])}
      />
    </QueryState>
  )
}
