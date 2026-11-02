//! The table's generated adapter and `register()`, from the spec's
//! table.proto, into OUT_DIR: the angzarr-embedded markup of the table
//! aggregate, every handler and applier delegating to its interface
//! (examples-proto's generated `domain::TableAggregate`) on its implementation
//! (agg-table's `Table`). The domain layer they name is examples-proto's.

const PROTOS: &[&str] = &["io/angzarr/examples/v1/table.proto"];
/// The domains a child process serves, whose stand-ins (their command and
/// event sets, as the policies name them) are generated.
const REMOTE: &[&str] = &["io/angzarr/examples/v1/player.proto"];
const INCLUDES: &[&str] = &["../../angzarr-project/proto"];

fn main() -> Result<(), angzarr_build::BuildError> {
    angzarr_build::adapters(PROTOS, INCLUDES, &[])?;
    angzarr_build::register(PROTOS, INCLUDES, &[])?;
    angzarr_build::stand_ins(REMOTE, INCLUDES)
}
