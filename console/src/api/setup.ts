import { invoke } from "@tauri-apps/api/core"
import type { ImportReportDto, ImportRequest } from "@/generated/sdk"

export type SetupDatabase = { kind: "sqlite"; path: string } | { kind: "url"; dsn: string }

/** Host-only startup messages; available before the management instance exists. */
export type SetupStatus = {
  required: boolean
  databaseKinds: Array<"sqlite" | "postgres" | "mysql">
  canChooseDataDir: boolean
  canAutoStart: boolean
  started: boolean
  completed: boolean
  dataDir: string
  host: string
  port: number
  adminUser: string
  autoStart: boolean
  tray: boolean
  closeToTray: boolean
  startHidden: boolean
  language: string
  database: SetupDatabase
}
export type SetupRequest = {
  dataDir: string
  host: string
  port: number
  adminUser: string
  password: string
  apiKey: string | null
  autoStart: boolean
  tray: boolean
  closeToTray: boolean
  startHidden: boolean
  language: string
  database: SetupDatabase
  import: ImportRequest | null
}
export type SetupResult = { existingAdminPreserved: boolean; baseUrl: string; apiKey: string; importReport: ImportReportDto | null }

export async function shellCall<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try { return await invoke<T>(command, args) }
  catch (error) {
    if (error && typeof error === "object" && "message" in error) throw new Error(String(error.message), { cause: error })
    throw error instanceof Error ? error : new Error(String(error), { cause: error })
  }
}
export const setupStatus = () => shellCall<SetupStatus>("desktop_setup_status")
export const completeSetup = (request: SetupRequest) => shellCall<SetupResult>("desktop_setup_complete", { request })
export const pickDataDirectory = () => shellCall<string | null>("desktop_setup_pick_directory")

type AndroidSetup = {
  setupFinished(): void
  permissionStatus(): string
  requestNotifications(): void
  requestBackground(): void
}
export function androidSetup(): AndroidSetup | undefined {
  return (window as { GproxyFiles?: AndroidSetup }).GproxyFiles
}

export function validListeningAddress(value: string): boolean {
  const host = value.trim()
  const parts = host.split(".")
  if (parts.length === 4 && parts.every(part => /^\d+$/.test(part) && String(Number(part)) === part && Number(part) <= 255)) return true
  if (!host.includes(":") || /[%[\]/?#]/.test(host)) return false
  try { return new URL(`http://[${host}]/`).hostname.startsWith("[") } catch { return false }
}
