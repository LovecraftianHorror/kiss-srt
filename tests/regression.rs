#[test]
fn ts_trailing_bytes() {
    assert!(kiss_srt::from_str("1\n00:00:00,000 --> 11:11:11,111A\nTrailing 'A'\n").is_err())
}

#[test]
fn empty_text_section() {
    assert!(kiss_srt::from_str("1\n00:00:00,000 --> 11:11:11,111\n\n").is_err());
}
