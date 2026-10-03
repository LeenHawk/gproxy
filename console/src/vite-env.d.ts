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
}
