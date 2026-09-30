import type { MouseEvent, ReactNode } from "react"
import { href, navigate } from "@/lib/router"

/**
 * An anchor that stays in the application.
 *
 * It is a real `<a href>` so the browser's own affordances — middle click,
 * "open in new tab", the status bar — keep working; only an unmodified left
 * click is intercepted.
 */
export function Link({ to, className, children, onClick, "aria-current": ariaCurrent }: {
  to: string
  className?: string
  children: ReactNode
  onClick?: () => void
  "aria-current"?: "page"
}) {
  const handle = (event: MouseEvent<HTMLAnchorElement>) => {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
    event.preventDefault()
    onClick?.()
    navigate(to)
  }
  return <a href={href(to)} className={className} onClick={handle} aria-current={ariaCurrent}>{children}</a>
}
