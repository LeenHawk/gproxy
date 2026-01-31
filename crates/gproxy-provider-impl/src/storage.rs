use gproxy_storage::TrafficStorage;

pub fn set_global_storage(storage: TrafficStorage) {
    gproxy_storage::set_global_storage(storage);
}

pub fn global_storage() -> Option<TrafficStorage> {
    gproxy_storage::global_storage()
}
