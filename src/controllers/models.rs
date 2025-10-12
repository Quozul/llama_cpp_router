use crate::services::backend_server_manager::BackendServerManagerState;
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
    fn new(id: impl ToString, owned_by: impl ToString) -> Self {
        Self {
            id: id.to_string(),
            object: "model".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            owned_by: owned_by.to_string(),
        }
    }
}

pub async fn get_models(
    backend_server_manager: BackendServerManagerState,
) -> Json<ModelListResponse> {
    let manager = backend_server_manager.lock().await;
    let models = manager
        .get_all_models()
        .iter()
        .map(|model| Model::new(&model.model_name, "llamacpp"))
        .collect();

    Json(ModelListResponse {
        object: "list".to_string(),
        data: models,
    })
}
