import { useEffect } from "react"

export function usePageTitle(title: string, instance = "GPROXY") {
  useEffect(() => { document.title = `${title} · ${instance}` }, [title, instance])
}
