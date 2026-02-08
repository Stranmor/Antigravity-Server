// OpenAI models listing
use super::*;

pub async fn handle_list_models(State(state): State<AppState>) -> impl IntoResponse {
    use std::collections::HashSet;

    let mut model_ids: HashSet<String> = HashSet::new();

    // 1. Real models from loaded accounts
    for model in state.token_manager.get_all_available_models() {
        let _: bool = model_ids.insert(model);
    }

    // 2. Custom mapping keys
    {
        let mapping = state.custom_mapping.read().await;
        for key in mapping.keys() {
            let _: bool = model_ids.insert(key.clone());
        }
    }

    // 3. Image model variants
    let base = "gemini-3-pro-image";
    let resolutions = ["", "-2k", "-4k"];
    let ratios = ["", "-1x1", "-4x3", "-3x4", "-16x9", "-9x16", "-21x9"];
    for res in resolutions {
        for ratio in ratios {
            let mut id = base.to_owned();
            id.push_str(res);
            id.push_str(ratio);
            let _: bool = model_ids.insert(id);
        }
    }

    let mut sorted_ids: Vec<String> = model_ids.into_iter().collect();
    sorted_ids.sort();

    let data: Vec<_> = sorted_ids
        .into_iter()
        .map(|id| {
            json!({
                "id": id,
                "object": "model",
                "created": 1_706_745_600,
                "owned_by": "antigravity"
            })
        })
        .collect();

    Json(json!({
        "object": "list",
        "data": data
    }))
}
