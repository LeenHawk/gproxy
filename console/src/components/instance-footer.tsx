import { useQuery } from "@tanstack/react-query"
import { INFO_KEY, instanceInfo } from "@/api/settings"

export function InstanceFooter() {
  const { data } = useQuery({ queryKey: INFO_KEY, queryFn: instanceInfo })
  if (!data?.footerText) return null

  return (
    <footer className="px-4 py-3 text-center text-sm wrap-anywhere text-muted-foreground">
      {data.footerUrl ? (
        <a className="underline underline-offset-4 hover:text-foreground" href={data.footerUrl}>
          {data.footerText}
        </a>
      ) : data.footerText}
    </footer>
  )
}
