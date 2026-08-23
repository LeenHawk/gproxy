mod files;
mod helpers;
mod local;
mod pagination;
mod skills;
mod table;

pub(super) fn table() -> gproxy_channel_api::SurfaceTable {
    table::table()
}
