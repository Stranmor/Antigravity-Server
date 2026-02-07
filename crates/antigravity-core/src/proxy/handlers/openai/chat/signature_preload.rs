use crate::proxy::mappers::openai::OpenAIRequest;
use crate::proxy::SignatureCache;

pub(super) async fn preload_signatures(openai_req: &OpenAIRequest) {
    let content_hashes: Vec<String> = openai_req
        .messages
        .iter()
        .filter(|m| m.role == "assistant")
        .filter_map(|m| m.reasoning_content.as_ref())
        .filter(|rc| !rc.is_empty())
        .map(|rc| SignatureCache::compute_content_hash(rc))
        .collect();

    if !content_hashes.is_empty() {
        SignatureCache::global().preload_signatures_from_db(&content_hashes).await;
    }
}
