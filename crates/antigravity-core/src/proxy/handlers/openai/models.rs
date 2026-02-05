// OpenAI models listing
use super::*;

pub async fn handle_list_models(State(state): State<AppState>) -> impl IntoResponse {
    use crate::proxy::common::model_mapping::get_all_dynamic_models;
    use std::collections::HashSet;

    let mut model_ids: HashSet<String> = HashSet::new();

    let account_models = state.token_manager.get_all_available_models();
    if account_models.is_empty() {
        let fallback = get_all_dynamic_models(&state.custom_mapping).await;
        for model in fallback {
            let _: bool = model_ids.insert(model);
        }
    } else {
        for model in account_models {
            let _: bool = model_ids.insert(model);
        }
        {
            let mapping = state.custom_mapping.read().await;
            for key in mapping.keys() {
                let _: bool = model_ids.insert(key.clone());
            }
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
