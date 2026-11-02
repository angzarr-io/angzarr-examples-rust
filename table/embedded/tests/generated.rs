//! The table built from the generated domain layer, not hand-written
//! wiring (red/table-generated; the plan is `rust-table-generation.plan.md`,
//! session inept-sixth-stick): each test fails if the file or type it names
//! comes back. The behaviour the refactor kept is pinned in
//! `src/table_aggregate.test.rs` (seed 21, the rejection codes' keys and
//! reasons).

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

/// The table's interface is the generated domain layer's
/// (`examples_proto::domain::TableAggregate`), regenerated with the protos:
/// no hand-kept copy of it remains beside its implementation (item S).
#[test]
fn the_tables_interface_is_generated_not_kept_by_hand() {
    let interface = Path::new(env!("CARGO_MANIFEST_DIR")).join("../agg/src/table_aggregate.rs");
    assert!(
        !interface.exists(),
        "{} is a hand-kept interface; the domain layer generates it",
        interface.display()
    );
}

/// The generated modules carry the allowances generated code needs, so no
/// include site of this workspace carries one by hand (item S).
#[test]
fn no_include_site_carries_an_allowance_for_generated_code() {
    for path in [
        crate_file("src/lib.rs"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../proto/src/lib.rs"),
    ] {
        let source = std::fs::read_to_string(&path).unwrap();
        for allowance in ["clippy::result_large_err", "clippy::excessive_nesting"] {
            assert!(
                !source.contains(allowance),
                "{} allows {allowance} by hand",
                path.display()
            );
        }
    }
}
