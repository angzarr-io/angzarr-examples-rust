use super::*;

#[test]
fn eu_1442_an_existing_table_is_refused_as_a_failed_precondition() {
    let code = RejectionCode::TableAlreadyExists;
    assert_eq!(code.reason(), "TABLE_ALREADY_EXISTS");
    assert_eq!(code.status(), StatusCode::FailedPrecondition);
}

#[test]
fn eu_1441_an_invalid_configuration_is_refused_as_an_invalid_argument() {
    let code = RejectionCode::InvalidTableConfig;
    assert_eq!(code.reason(), "INVALID_TABLE_CONFIG");
    assert_eq!(code.status(), StatusCode::InvalidArgument);
}

/// table.proto's rejection list, by status class.
#[test]
fn every_reason_is_the_one_table_proto_lists_with_its_status() {
    use StatusCode::{FailedPrecondition as Fp, InvalidArgument as Ia};
    let listed = [
        ("TABLE_ALREADY_EXISTS", Fp),
        ("INVALID_TABLE_CONFIG", Ia),
        ("TABLE_NOT_FOUND", Fp),
        ("SEAT_TAKEN", Fp),
        ("PLAYER_ALREADY_SEATED", Fp),
        ("NOT_SEATED", Fp),
        ("ROUND_IN_PROGRESS", Fp),
        ("ALREADY_BET", Fp),
        ("INSUFFICIENT_STACK", Fp),
        ("NO_BETS", Fp),
        ("NO_ROUND_IN_PROGRESS", Fp),
        ("NOT_YOUR_TURN", Fp),
        ("DOUBLE_NOT_ALLOWED", Fp),
        ("SEAT_OUT_OF_RANGE", Ia),
        ("BUY_IN_OUT_OF_RANGE", Ia),
        ("BET_OUT_OF_RANGE", Ia),
        ("BET_NOT_EVEN", Ia),
    ];
    let actual: Vec<_> = RejectionCode::ALL
        .iter()
        .map(|c| (c.reason(), c.status()))
        .collect();
    assert_eq!(actual, listed);
}
