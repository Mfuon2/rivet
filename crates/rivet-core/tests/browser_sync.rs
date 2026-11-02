use rivet_core::{DownloadRequest, native, queue::Queue};
use tempfile::TempDir;
fn request(value: serde_json::Value) -> Vec<u8> {
    let data = serde_json::to_vec(&value).unwrap();
    let mut bytes = (data.len() as u32).to_le_bytes().to_vec();
    bytes.extend(data);
    bytes
}
#[test]
fn sync_is_bounded_queue_only_and_does_not_disclose_other_history_or_paths() {
    let temp = TempDir::new().unwrap();
    let queue = Queue::open(temp.path().join("queue.db")).unwrap();
    let id = queue
        .enqueue(
            DownloadRequest::new(
                "https://example.com/file?secret=private".into(),
                temp.path().join("Visible name.mp4"),
            ),
            0,
        )
        .unwrap();
    queue
        .enqueue(
            DownloadRequest::new(
                "https://example.com/secret".into(),
                temp.path().join("Unrelated history.txt"),
            ),
            0,
        )
        .unwrap();
    let mut output = Vec::new();
    let mut launches = 0;
    native::serve_with_actions(
        &mut request(serde_json::json!({"type":"sync","ids":[id]})).as_slice(),
        &mut output,
        &queue,
        &temp.path().join("downloads"),
        || {},
        || {
            launches += 1;
            Ok(())
        },
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output[4..]).unwrap();
    assert_eq!(value["protocol"], 1);
    assert_eq!(value["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(value["jobs"][0]["name"], "Visible name.mp4");
    assert_eq!(launches, 0);
    assert!(!temp.path().join("downloads").exists());
    let serialized = String::from_utf8(output[4..].to_vec()).unwrap();
    for private in [
        "secret=private",
        "Unrelated history",
        temp.path().to_str().unwrap(),
    ] {
        assert!(!serialized.contains(private));
    }
    for ids in [vec!["../private".to_string()], vec![id; 41]] {
        let mut output = Vec::new();
        native::serve(
            &mut request(serde_json::json!({"type":"sync","ids":ids})).as_slice(),
            &mut output,
            &queue,
            temp.path(),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&output[4..]).unwrap();
        assert_eq!(value["ok"], false);
    }
}
