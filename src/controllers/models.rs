use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ModelListResponse {
    object: String,
    data: Vec<Model>,
}

#[derive(Debug, Serialize)]
pub struct Model {
    id: String,
    object: String,
    created: u64,
    owned_by: String,
}

impl Model {
    fn new(id: String, owned_by: String) -> Self {
        Self {
            id,
            object: "model".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            owned_by,
        }
    }
}

pub async fn get_models() -> Json<ModelListResponse> {
    let models = vec![
        Model::new("gpt-3.5-turbo".to_string(), "openai".to_string()),
        Model::new("gpt-3.5-turbo-16k".to_string(), "openai".to_string()),
        Model::new("gpt-4".to_string(), "openai".to_string()),
        Model::new("text-davinci-003".to_string(), "openai".to_string()),
        Model::new("text-davinci-002".to_string(), "openai".to_string()),
        Model::new("text-curie-001".to_string(), "openai".to_string()),
        Model::new("text-babbage-001".to_string(), "openai".to_string()),
        Model::new("text-ada-001".to_string(), "openai".to_string()),
    ];

    Json(ModelListResponse {
        object: "list".to_string(),
        data: models,
    })
}
