//! The table's generated adapter and `register()`, from the spec's
//! table.proto, into OUT_DIR: the angzarr-embedded markup of the table
//! aggregate, every handler and applier delegating to its interface
//! (agg-table's `table_aggregate::TableAggregate`) on its implementation
//! (agg-table's `Table`). The domain layer they name is examples-proto's.

const PROTOS: &[&str] = &["io/angzarr/examples/v1/table.proto"];
const INCLUDES: &[&str] = &["../../angzarr-project/proto"];

fn main() -> Result<(), angzarr_build::BuildError> {
    angzarr_build::adapters(PROTOS, INCLUDES, &[])?;
    angzarr_build::register(PROTOS, INCLUDES, &[])
}
