//! Grounding metadata types for web search augmented responses.
//!
//! This module defines the structures for grounding metadata returned
//! by Gemini when web search is used to augment responses.

use serde::{Deserialize, Serialize};

/// Metadata about grounding sources used in a response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingMetadata {
    /// Web search queries that were executed.
    #[serde(rename = "webSearchQueries")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search_queries: Option<Vec<String>>,
    /// Chunks of grounding information from web sources.
    #[serde(rename = "groundingChunks")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_chunks: Option<Vec<GroundingChunk>>,
    /// Support information linking response to sources.
    #[serde(rename = "groundingSupports")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_supports: Option<Vec<GroundingSupport>>,
    /// Entry point for search results.
    #[serde(rename = "searchEntryPoint")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_entry_point: Option<SearchEntryPoint>,
}

/// A chunk of grounding information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingChunk {
    /// Web source for this chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<WebSource>,
}

/// A web source used for grounding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSource {
    /// URI of the web source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// Title of the web page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Support information linking response segments to sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingSupport {
    /// Text segment in the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment: Option<TextSegment>,
    /// Indices of grounding chunks that support this segment.
    #[serde(rename = "groundingChunkIndices")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_chunk_indices: Option<Vec<i32>>,
    /// Confidence scores for each supporting chunk.
    #[serde(rename = "confidenceScores")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_scores: Option<Vec<f64>>,
}

/// A segment of text in the response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSegment {
    /// Start index of the segment.
    #[serde(rename = "startIndex")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_index: Option<i32>,
    /// End index of the segment.
    #[serde(rename = "endIndex")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_index: Option<i32>,
    /// Text content of the segment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Entry point for search results display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchEntryPoint {
    /// Rendered HTML content for display.
    #[serde(rename = "renderedContent")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_content: Option<String>,
}
