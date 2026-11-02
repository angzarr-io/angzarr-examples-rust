//! The generated adapters and `register()` of the table with wallets and
//! buy-ins, from the spec's protos, into OUT_DIR: the angzarr-embedded
//! markup of the table, the wallet, the buy-in process manager and the five
//! policies between them, every handler delegating to its interface
//! (examples-proto's generated domain layer's) on its implementation
//! (agg-table, agg-wallet, pm-buy-in, buy-in-policies, wallet-policies).
//! The player bot is not part of it: its domain is not among the protos, so
//! the policies to and from it are not generated here (table-embedded has
//! them).

const PROTOS: &[&str] = &[
    "io/angzarr/examples/v1/table.proto",
    "io/angzarr/examples/v1/wallet.proto",
    "io/angzarr/examples/v1/buy_in.proto",
    "io/angzarr/examples/v1/policies.proto",
];
const INCLUDES: &[&str] = &["../../angzarr-project/proto"];

fn main() -> Result<(), angzarr_build::BuildError> {
    angzarr_build::adapters(PROTOS, INCLUDES, &[])?;
    angzarr_build::register(PROTOS, INCLUDES, &[])
}
