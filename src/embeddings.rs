use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
}

/// Generate an embedding vector for the given text using Ollama's embedding API
pub async fn generate_embedding(
    ollama_host: &str,
    embedding_model: &str,
    text: &str,
) -> Result<Vec<f32>> {
    let url = format!("{}/api/embeddings", ollama_host);

    let request = EmbeddingRequest {
        model: embedding_model.to_string(),
        prompt: text.to_string(),
    };

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("Embedding API error ({}): {}", status, body));
    }

    let embedding_response: EmbeddingResponse = response.json().await?;

    Ok(embedding_response.embedding)
}

/// Generate embeddings for multiple texts in batch
#[allow(dead_code)]
pub async fn generate_embeddings_batch(
    ollama_host: &str,
    embedding_model: &str,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    let mut embeddings = Vec::new();

    for text in texts {
        let embedding = generate_embedding(ollama_host, embedding_model, text).await?;
        embeddings.push(embedding);
    }

    Ok(embeddings)
}

/// Calculate cosine similarity between two embedding vectors
#[allow(dead_code)]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![1.0, 0.0, 0.0];
        let d = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&c, &d) - 0.0).abs() < 0.001);
    }
}
