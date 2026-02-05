use super::request_executor::build_url;

#[test]
fn test_build_url() {
    let base_url = "https://cloudcode-pa.googleapis.com/v1internal";

    let url1 = build_url(base_url, "generateContent", None);
    assert_eq!(url1, "https://cloudcode-pa.googleapis.com/v1internal:generateContent");

    let url2 = build_url(base_url, "streamGenerateContent", Some("alt=sse"));
    assert_eq!(
        url2,
        "https://cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse"
    );
}
