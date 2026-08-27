/// <reference types="vite/client" />

declare const __GPROXY_VERSION__: string
declare const __GPROXY_BUILD_HASH__: string

interface GproxyBuildInfo {
  version: string
  channel: string
  buildHash: string
  installationKind: string
}

interface Window {
  __GPROXY_BUILD_INFO__?: GproxyBuildInfo
  __GPROXY_ANNOUNCEMENTS__?: Array<GproxyAnnouncement>
}

interface GproxyAnnouncement {
  id: string
  severity: "info" | "warning" | "critical"
  published_at: string
  expires_at?: string
  affects?: string
  content: Record<string, { title: string; body: string }>
}
