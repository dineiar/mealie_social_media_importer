use mealie_importer::mealie::{MealieError, RejectedBody};

#[test]
fn code_outside_the_module_can_build_a_rejected_error() {
    let error = MealieError::Rejected(RejectedBody::new(
        r#"{"detail":{"message":"AI services are not enabled"}}"#.to_owned(),
    ));

    let MealieError::Rejected(body) = &error else {
        panic!("built as Rejected");
    };
    assert!(body.as_str().contains("AI services are not enabled"));
    assert_eq!(error.to_string(), "Mealie rejected the request");
}
