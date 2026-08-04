# gproxy-protocol-macros

Implementation-only proc macros used by
[`gproxy-protocol`](https://crates.io/crates/gproxy-protocol) to provide checked
builders and the `wire!` construction helper for non-exhaustive provider wire
structs.

Applications should depend on `gproxy-protocol`, which re-exports the supported
construction surface. Direct use of this crate is not required.

## License

Licensed under the [MIT License](LICENSE).
