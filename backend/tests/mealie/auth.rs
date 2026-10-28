use std::sync::Arc;
use std::time::Duration;

use mealie_importer::auth::{Authenticator, USER_TTL};
use mealie_importer::mealie::{MealieApi, MealieError, MealieUser};
use secrecy::SecretString;
use serde_json::json;
use wiremock::matchers::{any, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{Call, Files, calls_to, client, key, kind, serve, user_json, users_ok};

type Lookup = Result<MealieUser, Arc<MealieError>>;

fn authenticator(server: &MockServer) -> Authenticator {
    Authenticator::new(Arc::new(client(server)), USER_TTL)
}

async fn accepts_the_key_once_then(status: u16) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/users/self"))
        .respond_with(users_ok())
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;
    server
}

fn rejected(lookup: &Lookup) -> bool {
    matches!(lookup, Err(error) if matches!(**error, MealieError::Unauthorized))
}

#[tokio::test]
async fn a_second_lookup_within_the_ttl_makes_no_users_self_call() {
    let server = serve(users_ok()).await;
    let auth = authenticator(&server);

    let first = auth.user(&key()).await.unwrap();
    let second = auth.user(&key()).await.unwrap();

    assert_eq!(first.id, "eaab0b0e-0df9-402c-8f5c-90cced537b01");
    assert_eq!(second.id, first.id);
    assert_eq!(calls_to(&server, "/api/users/self").await, 1);
}

#[tokio::test]
async fn each_key_gets_its_own_user() {
    let server = MockServer::start().await;
    for (key, id) in [("key-ana", "id-ana"), ("key-bo", "id-bo")] {
        let mut user = user_json();
        user["id"] = json!(id);
        Mock::given(header("authorization", format!("Bearer {key}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(user))
            .mount(&server)
            .await;
    }
    let auth = authenticator(&server);
    let [ana, bo] = ["key-ana", "key-bo"].map(SecretString::from);

    let ids = [
        auth.user(&ana).await.unwrap().id,
        auth.user(&bo).await.unwrap().id,
        auth.user(&ana).await.unwrap().id,
    ];

    assert_eq!(ids, ["id-ana", "id-bo", "id-ana"]);
    assert_eq!(calls_to(&server, "/api/users/self").await, 2);
}

#[tokio::test]
async fn the_cache_forgets_a_user_after_the_ttl() {
    let server = serve(users_ok()).await;
    let auth = Authenticator::new(Arc::new(client(&server)), Duration::from_millis(300));

    auth.user(&key()).await.unwrap();
    auth.user(&key()).await.unwrap();
    assert_eq!(calls_to(&server, "/api/users/self").await, 1);

    tokio::time::sleep(Duration::from_millis(400)).await;
    auth.user(&key()).await.unwrap();

    assert_eq!(calls_to(&server, "/api/users/self").await, 2);
}

#[tokio::test]
async fn first_lookups_that_overlap_share_one_users_self_call() {
    let server = serve(
        ResponseTemplate::new(200)
            .set_body_json(user_json())
            .set_delay(Duration::from_millis(150)),
    )
    .await;
    let auth = authenticator(&server);
    let key = key();

    let (a, b, c) = tokio::join!(auth.user(&key), auth.user(&key), auth.user(&key));

    assert!(a.is_ok() && b.is_ok() && c.is_ok());
    assert_eq!(calls_to(&server, "/api/users/self").await, 1);
}

#[tokio::test]
async fn a_failed_lookup_is_not_cached() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(users_ok())
        .mount(&server)
        .await;
    let auth = authenticator(&server);

    let first = auth.user(&key()).await;
    let second = auth.user(&key()).await;

    assert!(matches!(
        first.as_ref().map_err(|e| kind(e)),
        Err("Unreachable")
    ));
    assert!(second.is_ok());
}

#[tokio::test]
async fn a_401_or_403_to_any_call_makes_the_next_lookup_ask_mealie_again() {
    let files = Files::new();
    for status in [401, 403] {
        for call in Call::ALL {
            let server = accepts_the_key_once_then(status).await;
            let auth = authenticator(&server);
            auth.user(&key()).await.expect("the first lookup works");

            let error = call.run(&auth, &files).await.unwrap_err();

            assert_eq!(kind(&error), "Unauthorized", "{call:?} with {status}");
            assert!(
                rejected(&auth.user(&key()).await),
                "{call:?} with {status} left the key cached"
            );
        }
    }
}

#[tokio::test]
async fn any_other_failure_leaves_the_cached_user_alone() {
    let files = Files::new();
    for status in [400, 404, 500] {
        for call in Call::ALL {
            let server = accepts_the_key_once_then(status).await;
            let auth = authenticator(&server);
            auth.user(&key()).await.expect("the first lookup works");

            let error = call.run(&auth, &files).await.unwrap_err();

            assert_ne!(kind(&error), "Unauthorized", "{call:?} with {status}");
            assert!(
                auth.user(&key()).await.is_ok(),
                "{call:?} with {status} dropped the cached user"
            );
        }
    }
}

#[tokio::test]
async fn a_lookup_in_flight_when_the_key_is_evicted_is_not_cached() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/users/self"))
        .respond_with(users_ok().set_delay(Duration::from_millis(300)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/groups/self"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let auth = authenticator(&server);
    let key = key();

    let (lookup, revoked) = tokio::join!(auth.user(&key), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        auth.groups_self(&key).await
    });

    assert!(lookup.is_ok(), "the request that was in flight is answered");
    assert_eq!(kind(&revoked.unwrap_err()), "Unauthorized");
    auth.user(&key).await.expect("the key is looked up again");
    assert_eq!(calls_to(&server, "/api/users/self").await, 2);
}
