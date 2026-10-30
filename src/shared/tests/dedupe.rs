use super::common::*;

#[tokio::test]
async fn repeated_source_message_id_is_skipped_within_a_run() {
    let client = GraphqlClient::new("http://127.0.0.1:1/graphql".to_string()).unwrap();
    let mut context = context_with_session(client, true, 10, 0);

    let mut message = assistant_message("usage-only event");
    message.message_id = Some("5fa2468fc8c4272ee601fad2".to_string());

    let first = context.process_message(message.clone()).await.unwrap();
    assert!(first.processed);
    assert!(!first.skipped);

    let second = context.process_message(message).await.unwrap();
    assert!(!second.processed);
    assert!(second.skipped);

    let meta = context.session_cache.get(TEST_SESSION_ID).unwrap();
    assert_eq!(
        meta.msg_seq, 1,
        "duplicate must not consume a sequence slot"
    );
}

#[tokio::test]
async fn distinct_source_message_ids_are_not_deduped() {
    let client = GraphqlClient::new("http://127.0.0.1:1/graphql".to_string()).unwrap();
    let mut context = context_with_session(client, true, 10, 0);

    let mut first = assistant_message("one");
    first.message_id = Some("id-one".to_string());
    let mut second = assistant_message("two");
    second.message_id = Some("id-two".to_string());

    assert!(context.process_message(first).await.unwrap().processed);
    assert!(context.process_message(second).await.unwrap().processed);
    assert_eq!(
        context.session_cache.get(TEST_SESSION_ID).unwrap().msg_seq,
        2
    );
}

#[tokio::test]
async fn replay_identity_dedupes_without_changing_the_durable_message_id() {
    let client = GraphqlClient::new("http://127.0.0.1:1/graphql".to_string()).unwrap();
    let mut context = context_with_session(client, true, 10, 0);

    let mut original = assistant_message("same logical record");
    original.message_id = Some("legacy-raw-id-one".to_owned());
    original.transient_dedupe_id = Some("codex-replay/v1:logical-id".to_owned());
    let mut replay = original.clone();
    replay.message_id = Some("legacy-raw-id-two".to_owned());

    assert!(context.process_message(original).await.unwrap().processed);
    let duplicate = context.process_message(replay).await.unwrap();
    assert!(duplicate.skipped);
    assert_eq!(
        context.session_cache.get(TEST_SESSION_ID).unwrap().msg_seq,
        1
    );
}

#[tokio::test]
async fn messages_without_source_ids_are_never_deduped() {
    let client = GraphqlClient::new("http://127.0.0.1:1/graphql".to_string()).unwrap();
    let mut context = context_with_session(client, true, 10, 0);

    let message = assistant_message("identical content");
    assert!(
        context
            .process_message(message.clone())
            .await
            .unwrap()
            .processed
    );
    assert!(context.process_message(message).await.unwrap().processed);
    assert_eq!(
        context.session_cache.get(TEST_SESSION_ID).unwrap().msg_seq,
        2
    );
}
