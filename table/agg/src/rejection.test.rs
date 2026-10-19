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
