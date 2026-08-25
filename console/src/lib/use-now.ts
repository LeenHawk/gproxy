import { useSyncExternalStore } from "react"

const precisionSeconds = 60

function snapshot() {
  return Math.floor(Date.now() / 1000 / precisionSeconds) * precisionSeconds
}

function subscribe(listener: () => void) {
  const timer = window.setInterval(listener, precisionSeconds * 1000)
  window.addEventListener("focus", listener)
  return () => {
    window.clearInterval(timer)
    window.removeEventListener("focus", listener)
  }
}

export function useNow() {
  return useSyncExternalStore(subscribe, snapshot, snapshot)
}
