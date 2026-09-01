mod environments;
mod files;
mod helpers;
mod local;
mod oauth;
mod remote;
mod table;
mod tasks;
mod usage;

pub(super) fn table() -> gproxy_channel_api::SurfaceTable {
    table::table()
}
