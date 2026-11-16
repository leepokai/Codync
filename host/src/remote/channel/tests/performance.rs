use super::*;

#[tokio::test]
async fn an_authorized_phone_records_its_open_milestones_as_jsonl_lines() {
    let hub = temp_hub();
    let key = paired_phone(&hub).await;
    let mut phone = Phone::direct(&hub, key);
    assert!(matches!(phone.hello(&hub, false).await, Out::Msg(v) if v["t"] == "welcome"));

    let log = crate::service::data_dir().join("phone-performance.jsonl");
    let before = std::fs::read_to_string(&log).unwrap_or_default().lines().count();
    // The first milestones of a cold open: the link ready, then the hello.
    for (phase, ms) in [("link_ready", 240), ("hello", 260)] {
        let reply = phone
            .call(
                1,
                "recordPhonePerformance",
                json!({"type": "open_timing", "platform": "ios", "phase": phase, "open": "cold", "ms": ms}),
            )
            .await;
        assert_eq!(reply["ok"], json!({}));
    }

    let text = std::fs::read_to_string(&log).expect("the measurements are logged");
    let lines: Vec<Value> = text.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(lines.len(), before + 2);
    for (line, (phase, ms)) in lines[before..].iter().zip([("link_ready", 240), ("hello", 260)]) {
        assert_eq!(line["type"], "open_timing");
        assert_eq!(line["platform"], "ios");
        assert_eq!(line["phase"], phase);
        assert_eq!(line["open"], "cold");
        assert_eq!(line["ms"], ms);
        assert!(line["at"].is_string());
    }
}
