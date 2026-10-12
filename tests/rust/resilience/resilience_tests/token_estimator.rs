#[test]
fn token_estimator_scales_proportionally_to_text_length() {
    let short = TokenEstimator::estimate_text("hello");
    let long = TokenEstimator::estimate_text(&"word ".repeat(200));
    assert!(
        long > short * 10,
        "long text should have much higher token estimate"
    );
}


#[test]
fn token_estimator_message_slice_sums_correctly() {
    let messages = vec![
        ChatMessage::new(MessageRole::User, "What is 2+2?"),
        ChatMessage::new(MessageRole::Assistant, "The answer is 4."),
    ];
    let total = TokenEstimator::estimate_messages(&messages);
    let manual: usize = messages.iter().map(TokenEstimator::estimate_message).sum();
    assert_eq!(total, manual);
}
