use super::*;
#[test]
fn split_utf8_records_and_final_tail() {
    let text = "{\"content\":\"é漢\"}\r\n{\"done\":true}";
    for split in 0..=text.len() {
        let mut parser = StreamLines::default();
        let mut records = parser.push(&text.as_bytes()[..split], false).unwrap();
        records.extend(parser.push(&text.as_bytes()[split..], true).unwrap());
        assert_eq!(records.len(), 2);
        let value: Value = serde_json::from_str(records[0].trim()).unwrap();
        assert_eq!(value["content"], "é漢");
    }
}
