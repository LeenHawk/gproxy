mod binding;
mod log;
mod quota;
mod usage;

use super::TableSpec;

pub(super) fn tables() -> impl Iterator<Item = &'static TableSpec> {
    quota::TABLES
        .iter()
        .chain(usage::TABLES)
        .chain(log::TABLES)
        .chain(binding::TABLES)
}
