use std::error::Error;
use std::time::Duration;

use mealie_importer::mealie::MealieError;
use wiremock::ResponseTemplate;

use crate::support::{
    Call, Files, KEY, authorization, bearer, capture_logs, client, client_for, refused_client,
    serve,
};

fn everything_said_about(error: &MealieError) -> String {
    let mut said = format!("{error} {error:?} {error:#?}");
    let mut source = error.source();
    while let Some(cause) = source {
        said.push_str(&format!(" {cause} {cause:?}"));
        source = cause.source();
    }
    said
}

#[tokio::test]
async fn the_key_stays_out_of_debug_output_errors_and_trace_logs() {
    let logs = capture_logs();
    let files = Files::new();
    let mut said = Vec::new();
    let mut sent_with_the_key = 0;

    for status in [200_u16, 400, 401, 404, 500] {
        let server = serve(ResponseTemplate::new(status).set_body_string("not json")).await;
        let api = client(&server);
        said.push(format!("{api:?}"));
        for call in Call::ALL {
            if let Err(error) = call.run(&api, &files).await {
                said.push(everything_said_about(&error));
            }
        }
        sent_with_the_key += server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| authorization(request) == bearer())
            .count();
    }

    let slow = serve(ResponseTemplate::new(200).set_delay(Duration::from_millis(500))).await;
    let impatient = client_for(&slow.uri(), Duration::from_millis(50));
    for api in [&impatient, &refused_client()] {
        for call in Call::ALL {
            let error = call.run(api, &files).await.unwrap_err();
            said.push(everything_said_about(&error));
        }
    }

    let logged = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
    assert_eq!(
        sent_with_the_key,
        5 * Call::ALL.len(),
        "the key did travel as a bearer token"
    );
    assert!(
        logged.contains("mealie response"),
        "the client logs each response it gets"
    );
    assert!(!logged.contains(KEY), "a log line holds the key:\n{logged}");
    for text in said {
        assert!(
            !text.contains(KEY),
            "an error or Debug output holds the key: {text}"
        );
    }
}
