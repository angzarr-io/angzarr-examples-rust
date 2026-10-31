//! RED (red/table-generated): the table built from the generated domain
//! layer, not hand-written wiring. The plan is
//! `rust-table-generation.plan.md` (session inept-sixth-stick); each test
//! fails while the file or type it names is still here, and passes once
//! the refactor removes it. The behaviour the refactor must keep is
//! characterized in `src/table_aggregate.test.rs` (seed 21, the rejection
//! codes' wire strings).

use std::path::{Path, PathBuf};

fn crate_file(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn agg_file(path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../agg")
        .join(path);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The domain types are angzarr-build's `markup_domain`, generated into
/// OUT_DIR from the spec's protos; nothing restates them by hand.
#[test]
fn the_domain_layer_is_generated_not_hand_written() {
    let domain = crate_file("src/domain.rs");
    assert!(
        !domain.exists(),
        "{} is hand-written; it is generated from table.proto",
        domain.display()
    );
}

/// The rejection is the generated domain's (its codes declared in the
/// proto), so no hand-written mapping of agg-table's codes remains.
#[test]
fn the_rejection_mapping_is_not_hand_written() {
    let refused = crate_file("src/refused.rs");
    assert!(
        !refused.exists(),
        "{} restates the eighteen codes; they are declared in table.proto",
        refused.display()
    );
}

/// agg-table's create_table decides its two events as two values, not a
/// Vec its caller unpacks with `unreachable!`.
#[test]
fn create_table_decides_its_two_events_not_a_vec() {
    let create = agg_file("src/create_table.rs");
    assert!(
        !create.contains("Vec<TableEvent>"),
        "create_table returns a Vec"
    );
    let table = agg_file("src/table.rs");
    assert!(
        !table.contains("unreachable!(\"create_table decides"),
        "table.rs unpacks create_table's Vec"
    );
}

/// The legacy two-event `TableEvent` and its `apply` go: the generated
/// event enum is the table's.
#[test]
fn the_legacy_table_event_enum_is_gone() {
    let state = agg_file("src/state.rs");
    assert!(
        !state.contains("pub enum TableEvent"),
        "state.rs keeps TableEvent"
    );
}
