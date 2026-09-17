-- Values remain decimal strings. Lua floating-point never compares or returns
-- the counter itself, preserving exact values all the way through i64::MAX.
local function greater(a, b)
    return #a > #b or (#a == #b and a > b)
end
local key, op = KEYS[1], ARGV[1]
local value = redis.call('HGET', key, 'v')
local generation = redis.call('HGET', key, 't')
if (value and not generation) or (generation and not value) then
    return redis.error_reply('GPROXY_CACHE_CORRUPT incomplete counter')
end
if op == 'get' then
    if not value then return {0, '', ''} end
    return {1, value, generation}
end
if op == 'decrement' then
    if not value or generation ~= ARGV[2] then return {0, '', ''} end
    if greater(ARGV[3], value) then return {-1, '', ''} end
    redis.call('HINCRBY', key, 'v', '-' .. ARGV[3])
    return {1, redis.call('HGET', key, 'v'), generation}
end
local current = value or '0'
-- Caller computes limit-amount with checked integer arithmetic.
if ARGV[3] == '-1' or greater(current, ARGV[3]) then
    return {2, current, generation or ''}
end
if not value then
    redis.call('HSET', key, 'v', ARGV[2], 't', ARGV[4])
    redis.call('PEXPIRE', key, ARGV[5])
    generation = ARGV[4]
else
    redis.call('HINCRBY', key, 'v', ARGV[2])
end
return {1, redis.call('HGET', key, 'v'), generation}
