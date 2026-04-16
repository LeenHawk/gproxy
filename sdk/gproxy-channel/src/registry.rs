use std::collections::HashMap;

use crate::routing::RoutingTable;

/// Registration entry for automatic channel discovery via `inventory`.
pub struct ChannelRegistration {
    /// Channel identifier.
    pub id: &'static str,
    /// Factory function returning the channel's default routing table.
    pub routing_table_fn: fn() -> RoutingTable,
}

inventory::collect!(ChannelRegistration);

impl ChannelRegistration {
    /// Create a registration for a channel type.
    pub const fn new(id: &'static str, routing_table_fn: fn() -> RoutingTable) -> Self {
        Self {
            id,
            routing_table_fn,
        }
    }
}

/// Registry of all available channels, built from `inventory` at startup.
pub struct ChannelRegistry {
    channels: HashMap<&'static str, ChannelRegistration>,
}

impl ChannelRegistry {
    /// Collect all registered channels.
    pub fn collect() -> Self {
        let mut channels = HashMap::new();
        for reg in inventory::iter::<ChannelRegistration> {
            channels.insert(
                reg.id,
                ChannelRegistration {
                    id: reg.id,
                    routing_table_fn: reg.routing_table_fn,
                },
            );
        }
        Self { channels }
    }

    /// Look up a channel by ID.
    pub fn get(&self, id: &str) -> Option<&ChannelRegistration> {
        self.channels.get(id)
    }

    /// List all registered channel IDs.
    pub fn channel_ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.channels.keys().copied()
    }

    /// Get the default routing table for a channel.
    pub fn routing_table(&self, id: &str) -> Option<RoutingTable> {
        self.channels.get(id).map(|reg| (reg.routing_table_fn)())
    }
}
