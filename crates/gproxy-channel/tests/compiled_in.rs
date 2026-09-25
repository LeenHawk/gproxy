//! The list every host registers, against the channels themselves.
//!
//! `channels::compiled_in` is generated beside the module declarations, so a
//! channel cannot be compiled in and left out of it. What a macro cannot
//! check is that two channels claim different ids — that is a collision
//! between crates' worth of code, and it surfaces as a registry error at
//! startup rather than at compile time. So this registers the whole list.

use gproxy_channel::{ChannelRegistry, channels::compiled_in};

#[test]
fn every_compiled_channel_registers_under_an_id_of_its_own() {
    let mut registry = ChannelRegistry::new();
    for channel in compiled_in() {
        let id = channel.id().to_owned();
        let descriptor = channel.descriptor();
        assert_eq!(
            descriptor.id, id,
            "{id} describes itself as {}",
            descriptor.id
        );
        registry
            .register(channel)
            .unwrap_or_else(|error| panic!("register {id}: {error}"));
    }
}

#[test]
fn optional_address_fields_have_channel_owned_placeholders() {
    for channel in compiled_in() {
        let descriptor = channel.descriptor();
        for field in descriptor.config_keys {
            let address = field.name.ends_with("_url")
                || field.name.ends_with("_host")
                || field.name == "issuer";
            if field.required || !address {
                continue;
            }
            // These origins depend on the attached credential or account plan.
            if field.name == "base_url"
                && matches!(descriptor.id, "kimi" | "copilotcli" | "opencode")
            {
                continue;
            }
            assert!(
                field.placeholder.is_some_and(|value| !value.is_empty()),
                "{}.{} has no default hint",
                descriptor.id,
                field.name
            );
        }
    }
}
