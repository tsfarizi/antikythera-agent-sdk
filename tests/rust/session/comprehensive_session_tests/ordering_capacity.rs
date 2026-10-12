

#[test]
fn test_message_history_ordering_on_concurrent_adds() {
    let manager = Arc::new(SessionManager::new());
    let session_id = manager.create_session("user", "gpt-4").unwrap();

    let barrier = Arc::new(Barrier::new(10));
    let mut handles = vec![];

    for thread_id in 0..10 {
        let manager_clone = manager.clone();
        let session_id_clone = session_id.clone();
        let barrier_clone = barrier.clone();

        let handle = thread::spawn(move || {
            barrier_clone.wait();

                for i in 0..10 {
                    let msg = Message::user(format!("t{}-m{}", thread_id, i));
                    manager_clone.add_message(&session_id_clone, msg).ok();
                }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let session = manager.get_session(&session_id).unwrap().unwrap();
    assert_eq!(session.messages.len(), 100);
}

#[test]
fn test_session_message_capacity() {
    let manager = SessionManager::new();
    let id = manager.create_session("user", "gpt-4").unwrap();

    for i in 0..10_000 {
        let msg = Message::user(format!("msg-{}", i));
        manager.add_message(&id, msg).ok();
    }

    let session = manager.get_session(&id).unwrap().unwrap();
    assert_eq!(session.messages.len(), 10_000);
}
