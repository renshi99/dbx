use dbx_core::connection::AppState;
use dbx_core::xxljob::XxlJobRequest;

#[tauri::command]
pub async fn xxljob_request(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    operation: String,
    request: XxlJobRequest,
) -> Result<serde_json::Value, String> {
    dbx_core::xxljob::execute(&state, &operation, request).await
}
