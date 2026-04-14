use std::collections::HashMap;

use crate::dispatch::DispatchTable;

/// Registration entry for automatic channel discovery via `inventory`.
pub struct ChannelRegistration {
    /// Channel identifier.
    pub id: &'static str,
    /// Factory function returning the channel's default dispatch table.
    pub dispatch_table_fn: fn() -> DispatchTable,
}

inventory::collect!(ChannelRegistration);

impl ChannelRegistration {
    /// Create a registration for a channel type.
    pub const fn new(id: &'static str, dispatch_table_fn: fn() -> DispatchTable) -> Self {
        Self {
            id,
            dispatch_table_fn,
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
                    dispatch_table_fn: reg.dispatch_table_fn,
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

    /// Get the default dispatch table for a channel.
    pub fn dispatch_table(&self, id: &str) -> Option<DispatchTable> {
        self.channels.get(id).map(|reg| (reg.dispatch_table_fn)())
    }
}
