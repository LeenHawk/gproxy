local key, op = KEYS[1], ARGV[1]
local token = redis.call('HGET', key, 't')
if op == 'get' then
    if not token then return {0, {}} end
    local value = redis.call('HGET', key, 'v')
    if not value then return redis.error_reply('GPROXY_CACHE_CORRUPT missing value') end
    return {1, {token, value}}
end
if op == 'delete' then return {redis.call('DEL', key), {}} end
if op == 'cas' then
    local expected = ARGV[2]
    if (expected == '' and token) or (expected ~= '' and token ~= expected) then
        return {0, {}}
    end
    if ARGV[3] == '' then redis.call('DEL', key); return {1, {}} end
end
-- put and matching cas: new version, value and expiry are one operation.
redis.call('HSET', key, 't', ARGV[3], 'v', ARGV[4])
redis.call('PEXPIRE', key, ARGV[5])
return {1, {ARGV[3]}}
