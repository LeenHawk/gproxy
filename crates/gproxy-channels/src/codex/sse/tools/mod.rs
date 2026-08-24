mod alias;
mod normalize;
mod patch;
mod shell;

use std::collections::BTreeMap;

use alias::Alias;

pub(in crate::codex) use shell::shell_action;

#[derive(Default)]
pub(super) struct ToolAliases {
    aliases: BTreeMap<u32, Alias>,
}
