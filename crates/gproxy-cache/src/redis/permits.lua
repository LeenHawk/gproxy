local key, op, owner = KEYS[1], ARGV[1], ARGV[2]
local time = redis.call('TIME')
local now = tonumber(time[1]) * 1000 + math.floor(tonumber(time[2]) / 1000)
local ttl = tonumber(ARGV[3])
if ttl and now + ttl > 9007199254740991 then
    return redis.error_reply('GPROXY_CACHE_INVALID expiry exceeds exact timestamp range')
end
redis.call('ZREMRANGEBYSCORE', key, '-inf', string.format('%.0f', now))
if op == 'acquire' then
    if redis.call('ZCARD', key) >= tonumber(ARGV[4]) then return 0 end
    redis.call('ZADD', key, string.format('%.0f', now + ttl), owner)
elseif op == 'renew' then
    if not redis.call('ZSCORE', key, owner) then return 0 end
    redis.call('ZADD', key, string.format('%.0f', now + ttl), owner)
elseif op == 'release' then
    if redis.call('ZREM', key, owner) == 0 then return 0 end
end
-- Short leases must never shorten the lifetime of longer-lived holders.
local tail = redis.call('ZRANGE', key, -1, -1, 'WITHSCORES')
if #tail > 0 then redis.call('PEXPIREAT', key, string.format('%.0f', tonumber(tail[2]))) end
return 1
