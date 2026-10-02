import { useRef } from "react"

/** Controlled dialogs can be opened without a Radix Trigger. Remember their
 * actual opener so Escape and Cancel return keyboard users to their place. */
export function useDialogFocus(onOpen?: (event: Event) => void, onClose?: (event: Event) => void) {
  const opener = useRef<HTMLElement | null>(null)
  return {
    onOpenAutoFocus(event: Event) {
      opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null
      onOpen?.(event)
    },
    onCloseAutoFocus(event: Event) {
      onClose?.(event)
      if (event.defaultPrevented) return
      const target = opener.current
      if (target?.isConnected && target !== document.body && !target.matches(":disabled")) {
        event.preventDefault()
        target.focus({ preventScroll: true })
      }
    },
  }
}
