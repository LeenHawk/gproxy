// Canonical edge environment and init-once adapter. Platform build scripts copy
// this file to a local `_shared.js`; edit this source, not generated copies.
export const REQUIRED_EDGE_ENV = Object.freeze([
  "TURSO_URL",
  "TURSO_TOKEN",
  "GPROXY_ADMIN_USER",
  "GPROXY_ADMIN_PASSWORD",
]);

export const OPTIONAL_EDGE_ENV = Object.freeze([
  "UPSTASH_URL",
  "UPSTASH_TOKEN",
  "GPROXY_MASTER_KEY",
  "GPROXY_LOG",
]);

function readEnv(getEnv, name) {
  const value = getEnv(name);
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

export function initGproxy(init, getEnv) {
  const env = Object.fromEntries(
    [...REQUIRED_EDGE_ENV, ...OPTIONAL_EDGE_ENV].map((name) => [
      name,
      readEnv(getEnv, name),
    ]),
  );
  for (const name of REQUIRED_EDGE_ENV) {
    if (!env[name]) {
      throw new Error(`missing required env var: ${name}`);
    }
  }

  return init(
    env.TURSO_URL,
    env.TURSO_TOKEN,
    env.UPSTASH_URL,
    env.UPSTASH_TOKEN,
    env.GPROXY_MASTER_KEY,
    env.GPROXY_ADMIN_USER,
    env.GPROXY_ADMIN_PASSWORD,
    env.GPROXY_LOG,
  );
}

export function createInitOnce(start) {
  let pending;
  return (...args) => {
    if (!pending) {
      pending = Promise.resolve().then(() => start(...args));
    }
    return pending;
  };
}
