use gproxy_channel::channel::QuotaScope;
use gproxy_core::{BlockSource, CredentialBlock, CredentialBlocks};
use gproxy_protocol::Operation;

fn block(scope: QuotaScope, operation: Option<Operation>, until_ms: i64) -> CredentialBlock {
    CredentialBlock {
        scope,
        operation,
        until_ms,
        source: BlockSource::RateLimited,
        observed_at_ms: 0,
    }
}

#[test]
fn family_prefix_blocks_only_that_family_and_unknown_blocks_everything() {
    let sonnet = block(
        QuotaScope::ModelPrefixes(vec!["claude-sonnet-4".into()]),
        None,
        100,
    );
    assert!(sonnet.applies_to(Some("claude-sonnet-4"), Operation::GenerateContent));
    assert!(sonnet.applies_to(Some("claude-sonnet-4-5"), Operation::GenerateContent));
    assert!(!sonnet.applies_to(Some("claude-sonnet-45"), Operation::GenerateContent));
    assert!(!sonnet.applies_to(Some("claude-opus-4"), Operation::GenerateContent));
    assert!(!sonnet.applies_to(None, Operation::ListModels));

    let unknown = block(QuotaScope::Unknown, None, 100);
    assert!(unknown.applies_to(Some("anything"), Operation::GenerateContent));
    assert!(!unknown.applies_to(None, Operation::ListModels));
}

#[test]
fn operation_filter_and_expiry_are_honoured_and_longest_block_wins() {
    let blocks = CredentialBlocks {
        blocks: vec![
            block(QuotaScope::All, Some(Operation::CreateImage), 500),
            block(QuotaScope::Models(vec!["m".into()]), None, 200),
            block(QuotaScope::All, None, 50),
        ],
        ..Default::default()
    };
    let hit = blocks
        .blocked_by(Some("m"), Operation::GenerateContent, 100)
        .unwrap();
    assert_eq!(hit.until_ms, 200);
    assert!(
        blocks
            .blocked_by(Some("other"), Operation::GenerateContent, 100)
            .is_none()
    );
    assert_eq!(
        blocks
            .blocked_by(Some("other"), Operation::CreateImage, 100)
            .unwrap()
            .until_ms,
        500
    );
    assert!(
        blocks
            .blocked_by(Some("m"), Operation::GenerateContent, 200)
            .is_none()
    );
}
