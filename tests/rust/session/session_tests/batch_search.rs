#[test]
fn test_batch_export_import() {
    let manager = SessionManager::new();
    let session1 = manager.create_session("user-1", "gpt-4").unwrap();
    let session2 = manager.create_session("user-2", "gpt-3.5").unwrap();

    manager
        .add_message(&session1, Message::user("Hello"))
        .unwrap();
    manager.add_message(&session2, Message::user("Hi")).unwrap();

    let sessions: Vec<_> = [
        manager.get_session(&session1).unwrap().unwrap(),
        manager.get_session(&session2).unwrap().unwrap(),
    ]
    .to_vec();

    let batch = BatchExport::from_sessions(sessions);
    assert_eq!(batch.session_count(), 2);

    let json_data = batch.to_json().unwrap();

    let imported = BatchExport::from_json(&json_data).unwrap();
    assert_eq!(imported.session_count(), 2);
}


#[test]
fn test_search_sessions() {
    let manager = SessionManager::new();

    let id1 = manager.create_session("user-1", "gpt-4").unwrap();
    manager.update_title(&id1, "Weather query").unwrap();

    let id2 = manager.create_session("user-1", "gpt-4").unwrap();
    manager.update_title(&id2, "Code review").unwrap();

    let results = manager.search_sessions("weather").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, Some("Weather query".to_string()));
}
