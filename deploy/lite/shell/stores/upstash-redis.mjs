// Vercel: Upstash Redis from the Marketplace, over its REST API.
//
// The exchange is one Lua script, which Redis runs atomically on a single
// primary: this is a real compare-and-exchange. Versions come from a counter
// that is never reset, so a deleted and recreated key has a new one.

const PREFIX = "gproxy-lite:";

// KEYS: the entry, the version counter. ARGV: expected version, value,
// expiry in ms; each empty for none.
const EXCHANGE = `
local current = redis.call('HGET', KEYS[1], 'version')
if (current or '') ~= ARGV[1] then return false end
if ARGV[2] == '' then
  redis.call('DEL', KEYS[1])
  return ''
end
local version = tostring(redis.call('INCR', KEYS[2]))
redis.call('HSET', KEYS[1], 'value', ARGV[2], 'version', version)
if ARGV[3] == '' then redis.call('PERSIST', KEYS[1]) else redis.call('PEXPIREAT', KEYS[1], ARGV[3]) end
return version`;

export function upstashRedisStore(url, token) {
  if (!url || !token) throw new Error("the Upstash Redis REST URL and token are not set");
  async function command(...parts) {
    const response = await fetch(url, {
      method: "POST",
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify(parts),
    });
    const answer = await response.json();
    if (!response.ok || answer.error) throw new Error(answer.error ?? `Redis answered ${response.status}`);
    return answer.result;
  }
  return {
    async get(key) {
      const [value, version] = await command("HMGET", PREFIX + key, "value", "version");
      return value === null ? null : { value, version };
    },
    async compareExchange(key, expected, value, expiresAtMs) {
      const version = await command(
        "EVAL", EXCHANGE, "2", PREFIX + key, `${PREFIX}sequence`,
        expected ?? "", value ?? "", expiresAtMs === null ? "" : String(Math.ceil(expiresAtMs)),
      );
      if (version === null) return null;
      return { version: version === "" ? null : version };
    },
  };
}
