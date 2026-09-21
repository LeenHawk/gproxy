//! v3's `i64` primary keys as v4's `String` ones.
//!
//! One rule, and it is the whole module: a v3 row with id `7` in table `t`
//! becomes the v4 id `v3-t-7`. Three things follow from that and nothing else
//! would give all three at once.
//!
//! - **Deterministic.** The same document imported twice mints the same ids,
//!   so a re-run after an interrupted import completes it instead of doubling
//!   it. (`crud::id_or_new` and the sdk's equivalent both take a supplied id
//!   verbatim, which is what makes that possible at all.)
//! - **Traceable.** An operator reading `v3-credentials-12` in a v4 console
//!   knows which v3 row it was, without a mapping table that has to be kept.
//! - **Disjoint.** v4 mints 16 random bytes as lowercase hex, so no minted id
//!   can ever collide with one of these, and `v3-` is a reliable answer to
//!   "did the migration write this row?".
//!
//! The table name is v3's, not v4's: the point is to name the row it came
//! *from*. Where one v3 row fans out into several v4 rows — a `quotas` row is
//! six budget columns, a `rules` row can be several replacements — the id takes
//! a suffix naming which part it is, and the suffix is derived from the data
//! rather than from a counter so it stays stable across runs.

/// The prefix every id this migration mints starts with.
pub const PREFIX: &str = "v3-";

/// `v3-{table}-{id}`.
pub fn id(table: &str, old: i64) -> String {
    format!("{PREFIX}{table}-{old}")
}

/// `v3-{table}-{id}-{part}`, for a v3 row that becomes more than one v4 row.
pub fn part(table: &str, old: i64, part: &str) -> String {
    format!("{PREFIX}{table}-{old}-{part}")
}

/// Whether a v4 id was minted by this migration.
pub fn is_migrated(id: &str) -> bool {
    id.starts_with(PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_names_the_v3_table_and_row_it_came_from() {
        assert_eq!(id("credentials", 12), "v3-credentials-12");
        assert_eq!(part("quotas", 2, "daily"), "v3-quotas-2-daily");
    }

    #[test]
    fn minting_is_deterministic() {
        assert_eq!(id("providers", 1), id("providers", 1));
        assert_ne!(id("providers", 1), id("providers", 2));
        assert_ne!(id("providers", 1), id("routes", 1));
    }

    #[test]
    fn a_migrated_id_is_recognisable_and_a_minted_one_is_not() {
        assert!(is_migrated(&id("users", 4)));
        // What `crud::random_id` produces: 16 bytes of lowercase hex.
        assert!(!is_migrated("0f1e2d3c4b5a69788796a5b4c3d2e1f0"));
    }

    /// A negative v3 id is not something v3 could produce — its keys are
    /// `AUTOINCREMENT` — but it must still not collide with a positive one.
    #[test]
    fn a_negative_id_is_still_its_own_id() {
        assert_ne!(id("users", -1), id("users", 1));
    }
}
