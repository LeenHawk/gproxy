import { shellCall } from "@/api/setup"

export type ShellPreferences = {
  autoStart: boolean
  tray: boolean
  closeToTray: boolean
  startHidden: boolean
  language: string
}
export type ShellPreferencesStatus = ShellPreferences & {
  platform: "desktop" | "android" | "ohos"
  ohos?: OhosStatus
  desktop: boolean
  canAutoStart: boolean
  startupError: string | null
  trayError: string | null
}
export const SHELL_PREFERENCES_KEY = ["shell", "preferences"] as const
export const shellPreferences = () => shellCall<ShellPreferencesStatus>("desktop_preferences_status")
export const saveShellPreferences = (preferences: ShellPreferences) =>
  shellCall<ShellPreferencesStatus>("desktop_preferences_save", { preferences })

export type OhosStatus = {
  supported: boolean
  deviceType: string
  enabled: boolean
  active: boolean
  cancelled: boolean
  error: string | null
  autoStart: boolean | null
  startupError: string | null
  startMinimized: boolean
  canMinimize: boolean
  canTray: boolean
  tray: boolean
  trayReady: boolean
  closeToTray: boolean
  trayError: string | null
}
export type OhosAction = "startup-settings" | "minimize-on" | "minimize-off" | "tray-on" | "tray-off" | "close-hide" | "close-quit"
export const ohosAction = (action: OhosAction) => shellCall<OhosStatus>("desktop_application_action", { action })
export const setOhosBackground = (enabled: boolean) => shellCall<OhosStatus>("desktop_background_set", { enabled })
