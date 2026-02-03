use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingMetadata {
    #[serde(rename = "webSearchQueries")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search_queries: Option<Vec<String>>,
    #[serde(rename = "groundingChunks")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_chunks: Option<Vec<GroundingChunk>>,
    #[serde(rename = "groundingSupports")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_supports: Option<Vec<GroundingSupport>>,
    #[serde(rename = "searchEntryPoint")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_entry_point: Option<SearchEntryPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<WebSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingSupport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment: Option<TextSegment>,
    #[serde(rename = "groundingChunkIndices")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_chunk_indices: Option<Vec<i32>>,
    #[serde(rename = "confidenceScores")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_scores: Option<Vec<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSegment {
    #[serde(rename = "startIndex")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_index: Option<i32>,
    #[serde(rename = "endIndex")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchEntryPoint {
    #[serde(rename = "renderedContent")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_content: Option<String>,
}
