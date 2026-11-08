//! Live-API smoke test: fetch a real release from GitHub.
//! Skippable offline: set GIRUST_SMOKE=0.

#[tokio::test]
async fn fetch_latest_release_from_github() {
    if std::env::var("GIRUST_SMOKE").as_deref() == Ok("0") {
        eprintln!("skipping smoke test (GIRUST_SMOKE=0)");
        return;
    }
    // Build a plain client; cairnwatch's net module needs the binary
    // crate, so tests here exercise via the lib target below.
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.github.com/repos/serde-rs/serde/releases/latest")
        .header("User-Agent", "Cairnwatch-smoke")
        .send()
        .await
        .expect("network request");
    assert!(resp.status().is_success(), "github api reachable: {}", resp.status());
    let json: serde_json::Value = resp.json().await.expect("json");
    let tag = json.get("tag_name").and_then(|v| v.as_str()).unwrap_or("");
    assert!(!tag.is_empty(), "latest release tag present");
    eprintln!("serde latest tag: {tag}");
}
