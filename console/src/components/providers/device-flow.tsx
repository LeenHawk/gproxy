import type { DeviceStartResponse } from "@/generated/DeviceStartResponse"
import { useMutation } from "@tanstack/react-query"
import { useEffect, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { pollDevice, startDevice } from "@/api/login"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"

type Props = {
  providerId: number
  label: string
  params: Record<string, string>
  disabled?: boolean
  onDone: () => void
}

type PollState = "pending" | "error" | "denied"

export function DeviceFlow({ providerId, label, params, disabled, onDone }: Props) {
  const { t } = useTranslation()
  const [session, setSession] = useState<DeviceStartResponse | null>(null)
  const [pollState, setPollState] = useState<PollState>("pending")
  const [pollRun, setPollRun] = useState(0)
  const onDoneRef = useRef(onDone)
  useEffect(() => {
    onDoneRef.current = onDone
  }, [onDone])
  const start = useMutation({
    mutationFn: () => startDevice({ provider_id: providerId, params, label: label.trim() || null }),
    onSuccess: (value) => {
      setPollState("pending")
      setSession(value)
    },
  })

  useEffect(() => {
    if (!session || pollState !== "pending") return
    let active = true
    let timer: ReturnType<typeof setTimeout>
    const tick = async () => {
      try {
        const value = await pollDevice({ login_session_id: session.login_session_id })
        if (!active) return
        if (value.status === "ready") {
          onDoneRef.current()
        } else if (value.status === "denied") {
          setPollState("denied")
        } else {
          timer = setTimeout(() => void tick(), Math.max(session.interval_secs, 2) * 1000)
        }
      } catch {
        if (active) setPollState("error")
      }
    }
    timer = setTimeout(() => void tick(), Math.max(session.interval_secs, 2) * 1000)
    return () => {
      active = false
      clearTimeout(timer)
    }
  }, [pollRun, pollState, session])

  if (!session) {
    return (
      <div className="flex flex-col gap-4">
        {start.isError ? <PollAlert state="start" /> : null}
        <Button type="button" onClick={() => start.mutate()} disabled={disabled || start.isPending}>
          {t(start.isPending ? "providers.login.starting" : "providers.login.start")}
        </Button>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-4">
      <Button asChild variant="outline">
        <a href={session.verification_uri} target="_blank" rel="noreferrer">
          {t("providers.login.openVerification")}
        </a>
      </Button>
      <div className="flex flex-col items-center gap-2 text-center">
        <p className="text-sm text-muted-foreground">{t("providers.login.deviceCode")}</p>
        <p className="machine-text text-2xl font-semibold tracking-widest">{session.user_code}</p>
        {pollState === "pending" ? <p className="text-sm text-muted-foreground">{t("providers.login.pending")}</p> : null}
      </div>
      {pollState === "error" ? <PollAlert state="poll" /> : null}
      {pollState === "denied" ? <PollAlert state="denied" /> : null}
      {pollState === "error" ? (
        <Button type="button" onClick={() => { setPollState("pending"); setPollRun((value) => value + 1) }}>
          {t("providers.login.retryPoll")}
        </Button>
      ) : null}
      {pollState === "denied" ? (
        <Button type="button" onClick={() => setSession(null)}>{t("providers.login.restart")}</Button>
      ) : null}
    </div>
  )
}

function PollAlert({ state }: { state: "start" | "poll" | "denied" }) {
  const { t } = useTranslation()
  return (
    <Alert variant="destructive">
      <AlertTitle>{t(`providers.login.errors.${state}Title`)}</AlertTitle>
      <AlertDescription>{t(`providers.login.errors.${state}Description`)}</AlertDescription>
    </Alert>
  )
}
