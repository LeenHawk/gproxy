pub const SQLITE_SQL: &[&str] = &[
    "ALTER TABLE quotas ADD COLUMN quota_daily TEXT",
    "ALTER TABLE quotas ADD COLUMN quota_weekly TEXT",
    "ALTER TABLE quotas ADD COLUMN quota_monthly TEXT",
    "ALTER TABLE quotas ADD COLUMN day_used TEXT NOT NULL DEFAULT '0'",
    "ALTER TABLE quotas ADD COLUMN day_anchor INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE quotas ADD COLUMN week_used TEXT NOT NULL DEFAULT '0'",
    "ALTER TABLE quotas ADD COLUMN week_anchor INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE quotas ADD COLUMN month_used TEXT NOT NULL DEFAULT '0'",
    "ALTER TABLE quotas ADD COLUMN month_anchor INTEGER NOT NULL DEFAULT 0",
];

pub const POSTGRES_SQL: &[&str] = &[
    "ALTER TABLE quotas ADD COLUMN quota_daily VARCHAR(64)",
    "ALTER TABLE quotas ADD COLUMN quota_weekly VARCHAR(64)",
    "ALTER TABLE quotas ADD COLUMN quota_monthly VARCHAR(64)",
    "ALTER TABLE quotas ADD COLUMN day_used VARCHAR(64) NOT NULL DEFAULT '0'",
    "ALTER TABLE quotas ADD COLUMN day_anchor BIGINT NOT NULL DEFAULT 0",
    "ALTER TABLE quotas ADD COLUMN week_used VARCHAR(64) NOT NULL DEFAULT '0'",
    "ALTER TABLE quotas ADD COLUMN week_anchor BIGINT NOT NULL DEFAULT 0",
    "ALTER TABLE quotas ADD COLUMN month_used VARCHAR(64) NOT NULL DEFAULT '0'",
    "ALTER TABLE quotas ADD COLUMN month_anchor BIGINT NOT NULL DEFAULT 0",
];

pub const MYSQL_SQL: &[&str] = &[
    "ALTER TABLE quotas ADD COLUMN quota_daily TEXT",
    "ALTER TABLE quotas ADD COLUMN quota_weekly TEXT",
    "ALTER TABLE quotas ADD COLUMN quota_monthly TEXT",
    "ALTER TABLE quotas ADD COLUMN day_used TEXT NOT NULL DEFAULT '0'",
    "ALTER TABLE quotas ADD COLUMN day_anchor BIGINT NOT NULL DEFAULT 0",
    "ALTER TABLE quotas ADD COLUMN week_used TEXT NOT NULL DEFAULT '0'",
    "ALTER TABLE quotas ADD COLUMN week_anchor BIGINT NOT NULL DEFAULT 0",
    "ALTER TABLE quotas ADD COLUMN month_used TEXT NOT NULL DEFAULT '0'",
    "ALTER TABLE quotas ADD COLUMN month_anchor BIGINT NOT NULL DEFAULT 0",
];
