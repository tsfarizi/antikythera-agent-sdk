

#[test]
fn test_concurrent_session_creation() {
    let manager = Arc::new(SessionManager::new());
    let thread_count = 10;
    let sessions_per_thread = 50;

    let mut handles = vec![];

    for thread_id in 0..thread_count {
        let manager_clone = manager.clone();
        let handle = thread::spawn(move || {
            for i in 0..sessions_per_thread {
                manager_clone.create_session(
                    format!("user-{}-{}", thread_id, i),
                    "gpt-4",
                ).unwrap();
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(manager.session_count().unwrap(), thread_count * sessions_per_thread);
}

#[test]
fn test_concurrent_message_addition() {
    let manager = Arc::new(SessionManager::new());
    let session_id = manager.create_session("user-123", "gpt-4").unwrap();

    let thread_count = 20;
    let messages_per_thread = 50;

    let mut handles = vec![];

    for thread_id in 0..thread_count {
        let manager_clone = manager.clone();
        let session_id_clone = session_id.clone();

        let handle = thread::spawn(move || {
            for msg_id in 0..messages_per_thread {
                let msg = Message::user(format!("t{}-m{}", thread_id, msg_id));
                manager_clone.add_message(&session_id_clone, msg).ok();
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let session = manager.get_session(&session_id).unwrap().unwrap();
    assert_eq!(session.messages.len(), thread_count * messages_per_thread);
}

#[test]
fn test_concurrent_read_write() {
    let manager = Arc::new(SessionManager::new());

    let mut handles = vec![];

    for i in 0..3 {
        let manager_clone = manager.clone();
        let handle = thread::spawn(move || {
            for j in 0..100 {
                manager_clone.create_session(format!("user-{}-{}", i, j), "gpt-4").unwrap();
            }
        });
        handles.push(handle);
    }

    for _ in 0..3 {
        let manager_clone = manager.clone();
        let handle = thread::spawn(move || {
            for _ in 0..200 {
                let _ = manager_clone.list_sessions();
                let _ = manager_clone.session_count();
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(manager.session_count().unwrap(), 300);
}
