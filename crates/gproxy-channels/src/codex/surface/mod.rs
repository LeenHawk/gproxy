mod environments;
mod files;
mod helpers;
mod local;
mod remote;
mod table;
mod tasks;

pub(super) fn table() -> gproxy_channel_api::SurfaceTable {
    table::table()
}
