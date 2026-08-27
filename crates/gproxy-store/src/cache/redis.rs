use std::time::Duration;

use gproxy_core::CacheBackend;
use gproxy_core::channel_api::BoxFuture;
use redis::aio::ConnectionManager;

use super::{error, ttl_millis};

type Error = gproxy_core::error::StoreError;

#[derive(Clone)]
pub struct RedisCache {
    connection: ConnectionManager,
}

impl RedisCache {
    pub async fn connect(url: &str) -> Result<Self, Error> {
        let client = redis::Client::open(url).map_err(|_| error("Redis", "configuration"))?;
        let connection = ConnectionManager::new(client)
            .await
            .map_err(|_| error("Redis", "connection"))?;
        Ok(Self { connection })
    }

    async fn command<T: redis::FromRedisValue>(
        &self,
        command: &mut redis::Cmd,
        operation: &'static str,
    ) -> Result<T, Error> {
        command
            .query_async(&mut self.connection.clone())
            .await
            .map_err(|_| error("Redis", operation))
    }
}

impl CacheBackend for RedisCache {
    fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<Vec<u8>>, Error>> {
        Box::pin(async move { self.command(redis::cmd("GET").arg(key), "get").await })
    }

    fn set<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<(), Error>> {
        Box::pin(async move {
            let mut command = redis::cmd("SET");
            command.arg(key).arg(value);
            let ttl = ttl_millis(ttl);
            if ttl > 0 {
                command.arg("PX").arg(ttl);
            }
            self.command(&mut command, "set").await
        })
    }

    fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), Error>> {
        Box::pin(async move {
            self.command::<u64>(redis::cmd("DEL").arg(key), "delete")
                .await?;
            Ok(())
        })
    }

    fn incr<'a>(
        &'a self,
        key: &'a str,
        by: i64,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<i64, Error>> {
        Box::pin(async move {
            let script = "local e=redis.call('EXISTS',KEYS[1]); local v=redis.call('INCRBY',KEYS[1],ARGV[1]); if e==0 and tonumber(ARGV[2])>0 then redis.call('PEXPIRE',KEYS[1],ARGV[2]); end; return v";
            self.command(
                redis::cmd("EVAL")
                    .arg(script)
                    .arg(1)
                    .arg(key)
                    .arg(by)
                    .arg(ttl_millis(ttl)),
                "increment",
            )
            .await
        })
    }

    fn compare_incr_and_set<'a>(
        &'a self,
        counter_key: &'a str,
        by: i64,
        state_key: &'a str,
        expected: Vec<u8>,
        state: Vec<u8>,
    ) -> BoxFuture<'a, Result<Option<i64>, Error>> {
        Box::pin(async move {
            let script = "if redis.call('GET',KEYS[2])~=ARGV[2] then return false end; local v=redis.call('INCRBY',KEYS[1],ARGV[1]); redis.call('SET',KEYS[2],ARGV[3]); return v";
            self.command(
                redis::cmd("EVAL")
                    .arg(script)
                    .arg(2)
                    .arg(counter_key)
                    .arg(state_key)
                    .arg(by)
                    .arg(expected)
                    .arg(state),
                "compare increment",
            )
            .await
        })
    }

    fn compare_and_swap<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Vec<u8>>,
        value: Option<Vec<u8>>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<bool, Error>> {
        Box::pin(async move {
            let script = "local c=redis.call('GET',KEYS[1]); if (ARGV[1]=='0' and c) or (ARGV[1]=='1' and c~=ARGV[2]) then return 0 end; if ARGV[3]=='1' then if tonumber(ARGV[5])>0 then redis.call('SET',KEYS[1],ARGV[4],'PX',ARGV[5]) else redis.call('SET',KEYS[1],ARGV[4]) end else redis.call('DEL',KEYS[1]) end; return 1";
            let expected_present = u8::from(expected.is_some());
            let value_present = u8::from(value.is_some());
            let result: i64 = self
                .command(
                    redis::cmd("EVAL")
                        .arg(script)
                        .arg(1)
                        .arg(key)
                        .arg(expected_present)
                        .arg(expected.unwrap_or_default())
                        .arg(value_present)
                        .arg(value.unwrap_or_default())
                        .arg(ttl_millis(ttl)),
                    "compare and swap",
                )
                .await?;
            Ok(result == 1)
        })
    }
}
