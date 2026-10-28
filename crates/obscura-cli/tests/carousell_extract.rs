use std::path::PathBuf;
use std::process::Command;

const EXTRACT_JS: &str = include_str!("../src/carousell.js");

fn extract(fixture: &str) -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture);
    let output = Command::new(env!("CARGO_BIN_EXE_obscura"))
        .args(["fetch", &format!("file://{}", path.display()), "--quiet", "--eval", EXTRACT_JS])
        .output()
        .expect("run obscura fetch");
    assert!(
        output.status.success(),
        "obscura fetch failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("extractor returns JSON")
}

#[test]
fn extracts_listing_cards_by_test_id() {
    let v = extract("carousell_search.html");
    assert_eq!(v["blocked"], false);
    let l = v["listings"].as_array().unwrap();
    assert_eq!(l.len(), 3);

    assert_eq!(l[0]["id"], "1287654321");
    assert_eq!(l[0]["title"], "iPhone 15 128GB Black");
    assert_eq!(l[0]["price"], "S$850");
    assert_eq!(l[0]["original_price"], "S$999");
    assert_eq!(l[0]["condition"], "Like new");
    assert_eq!(l[0]["seller"], "alice_sells");
    assert_eq!(l[0]["posted"], "2 days ago");
    assert!(l[0]["url"]
        .as_str()
        .unwrap()
        .ends_with("/p/iphone-15-128gb-black-1287654321/"));

    // Lazy-loaded image falls back to data-src.
    assert!(l[1]["image"].as_str().unwrap().ends_with("2_thumbnail.jpg"));
    assert_eq!(l[1]["price"], "S$1,420");

    assert_eq!(l[2]["price"], "Free");
    assert_eq!(l[2]["posted"], "yesterday");
}

#[test]
fn falls_back_to_product_links_without_test_ids() {
    let v = extract("carousell_search_no_testid.html");
    let l = v["listings"].as_array().unwrap();
    assert_eq!(l.len(), 2);
    assert_eq!(l[0]["id"], "1300000001");
    assert_eq!(l[0]["title"], "PS5 Slim Disc");
    assert_eq!(l[0]["price"], "RM1,650");
    assert_eq!(l[1]["seller"], "eve");
    assert_eq!(l[1]["condition"], "Well used");
}

#[test]
fn flags_bot_challenge_page() {
    let v = extract("carousell_challenge.html");
    assert_eq!(v["blocked"], true);
    assert!(v["listings"].as_array().unwrap().is_empty());
}
