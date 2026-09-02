export type BuildIdentity = { version: string; channel: string; hash: string; kind: string }

export function buildIdentity(): BuildIdentity {
  const build = window.__GPROXY_BUILD_INFO__
  return build
    ? { version: build.version, channel: build.channel, hash: build.buildHash.slice(0, 12), kind: build.installationKind }
    : { version: __GPROXY_VERSION__, channel: "development", hash: __GPROXY_BUILD_HASH__, kind: "source" }
}
