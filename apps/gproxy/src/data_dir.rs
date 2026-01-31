pub(crate) fn resolve_data_dir(cli_value: &str) -> String {
    if !cli_value.trim().is_empty() {
        return cli_value.to_string();
    }
    if let Ok(value) = std::env::var("GPROXY_DATA_DIR")
        && !value.trim().is_empty() {
            return value;
        }
    "./data".to_string()
}
