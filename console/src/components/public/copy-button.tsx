import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { copyText } from "@/lib/copy-text"

type CopyState = "idle" | "copied" | "failed"

export function CopyButton({ value, className }: { value: string; className?: string }) {
  const { t } = useTranslation()
  const [state, setState] = useState<CopyState>("idle")

  useEffect(() => {
    if (state === "idle") return
    const timer = window.setTimeout(() => setState("idle"), 1800)
    return () => window.clearTimeout(timer)
  }, [state])

  return (
    <Button
      size="sm"
      variant="outline"
      className={className}
      aria-live="polite"
      onClick={() => { void copyText(value).then(() => setState("copied"), () => setState("failed")) }}
    >
      {t(state === "copied" ? "public.connect.copied" : state === "failed" ? "public.connect.copyFailed" : "public.connect.copy")}
    </Button>
  )
}
