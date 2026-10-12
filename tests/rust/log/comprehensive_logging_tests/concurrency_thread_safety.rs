

#[test]
fn test_concurrent_logging_basic() {
    let logger = Arc::new(Logger::new("concurrent-test"));
    let thread_count = 10;
    let messages_per_thread = 100;

    let mut handles = vec![];

    for thread_id in 0..thread_count {
        let logger_clone = logger.clone();
        let handle = thread::spawn(move || {
            for msg_id in 0..messages_per_thread {
                logger_clone.info(format!("thread-{}-msg-{}", thread_id, msg_id));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(logger.len(), thread_count * messages_per_thread);
}

#[test]
/// All messages should fit in capacity.
///
/// At least some messages should be logged.
///
fn test_concurrent_logging_stress() {
    let logger = Arc::new(Logger::with_capacity("stress-test", 100_000));
    let thread_count = 50;
    let messages_per_thread = 500;

    let mut handles = vec![];

    for thread_id in 0..thread_count {
        let logger_clone = logger.clone();
        let handle = thread::spawn(move || {
            for msg_id in 0..messages_per_thread {
                let level = match msg_id % 4 {
                    0 => LogLevel::Debug,
                    1 => LogLevel::Info,
                    2 => LogLevel::Warn,
                    _ => LogLevel::Error,
                };
                logger_clone.log(level, format!("t{}-m{}", thread_id, msg_id));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    assert!(logger.len() <= 100_000);
    assert!(!logger.is_empty());
}

#[test]
fn test_concurrent_read_write() {
    let logger = Arc::new(Logger::with_capacity("read-write-test", 10_000));

    let mut handles = vec![];

    for thread_id in 0..5 {
        let logger_clone = logger.clone();
        let handle = thread::spawn(move || {
            for i in 0..200 {
                logger_clone.info(format!("writer-{}-msg-{}", thread_id, i));
            }
        });
        handles.push(handle);
    }

    for _ in 0..5 {
        let logger_clone = logger.clone();
        let handle = thread::spawn(move || {
            for _ in 0..100 {
                let _ = logger_clone.get_latest(10);
                let _ = logger_clone.get_logs(&LogFilter::new());
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    assert!(!logger.is_empty());
}

#[test]
fn test_concurrent_clear() {
    let logger = Arc::new(Logger::new("concurrent-clear"));

    for i in 0..100 {
        logger.info(format!("msg-{}", i));
    }

    let barrier = Arc::new(Barrier::new(5));
    let mut handles = vec![];

    for thread_id in 0..5 {
        let logger_clone = logger.clone();
        let barrier_clone = barrier.clone();

        let handle = thread::spawn(move || {
            barrier_clone.wait();

            if thread_id == 0 {
                logger_clone.clear();
            } else {
                let _ = logger_clone.get_latest(10);
                logger_clone.info(format!("post-clear-{}", thread_id));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}
