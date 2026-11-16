use super::*;

#[tokio::test]
async fn an_authorized_phone_records_performance_as_one_jsonl_line() {
    let hub = temp_hub();
    let key = paired_phone(&hub).await;
    let mut phone = Phone::direct(&hub, key);
    assert!(matches!(phone.hello(&hub, false).await, Out::Msg(v) if v["t"] == "welcome"));

    let log = crate::service::data_dir().join("phone-performance.jsonl");
    let before = std::fs::read_to_string(&log).unwrap_or_default().lines().count();
    let reply = phone
        .call(
            1,
            "recordPhonePerformance",
            json!({"type": "open_timing", "platform": "ios", "phase": "link_ready", "open": "cold", "ms": 240}),
        )
        .await;
    assert_eq!(reply["ok"], json!({}));

    let text = std::fs::read_to_string(&log).expect("the measurement is logged");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), before + 1);
    let line: Value = serde_json::from_str(lines.last().unwrap()).unwrap();
    assert_eq!(line["type"], "open_timing");
    assert_eq!(line["platform"], "ios");
    assert_eq!(line["phase"], "link_ready");
    assert_eq!(line["ms"], 240);
    assert!(line["at"].is_string());
}
