

#[test]
fn test_rapid_session_creation() {
    let manager = SessionManager::new();
    let start = Instant::now();

    for i in 0..10_000 {
        manager.create_session(format!("user-{}", i), "gpt-4").unwrap();
    }

    let elapsed = start.elapsed();
    let per_second = (10_000.0 / elapsed.as_secs_f64()) as u64;

    println!("Created 10k sessions in {:?} ({} sess/sec)", elapsed, per_second);
    assert!(elapsed.as_secs() < 10);
}

#[test]
fn test_rapid_message_addition() {
    let manager = SessionManager::new();
    let id = manager.create_session("user", "gpt-4").unwrap();

    let start = Instant::now();

    for i in 0..10_000 {
        let msg = Message::user(format!("msg-{}", i));
        manager.add_message(&id, msg).ok();
    }

    let elapsed = start.elapsed();
    let per_second = (10_000.0 / elapsed.as_secs_f64()) as u64;

    println!("Added 10k messages in {:?} ({} msg/sec)", elapsed, per_second);
    assert!(elapsed.as_secs() < 10);
}

#[test]
fn test_large_session_retrieval() {
    let manager = SessionManager::new();
    let id = manager.create_session("user", "gpt-4").unwrap();

    for i in 0..5_000 {
        let msg = Message::user(format!("msg-{}", i));
        manager.add_message(&id, msg).ok();
    }

    let start = Instant::now();
    let _session = manager.get_session(&id).unwrap().unwrap();
    let elapsed = start.elapsed();

    println!("Retrieved session with 5k messages in {:?}", elapsed);
    assert!(elapsed.as_millis() < 100);
}

#[test]
fn test_many_sessions_list() {
    let manager = SessionManager::new();

    for i in 0..1_000 {
        manager.create_session(format!("user-{}", i), "gpt-4").unwrap();
    }

    let start = Instant::now();
    let summaries = manager.list_sessions().unwrap();
    let elapsed = start.elapsed();

    assert_eq!(summaries.len(), 1_000);
    println!("Listed 1k sessions in {:?}", elapsed);
    assert!(elapsed.as_millis() < 500);
}
