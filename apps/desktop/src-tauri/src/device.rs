use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::AppError;

const DEVICE_FILE: &str = "device.json";

#[derive(Serialize, Deserialize)]
struct DeviceFile {
    device_id: String,
}

/// Прочитать device-id из app_data_dir/device.json или создать новый и записать.
/// См. M9.2 + раздел 6.1 паспорта. R1: сброс при переустановке принят осознанно.
pub async fn ensure_device_id(app_data_dir: &Path) -> Result<String, AppError> {
    let path = app_data_dir.join(DEVICE_FILE);
    match tokio::fs::read_to_string(&path).await {
        Ok(text) => {
            let parsed: DeviceFile = serde_json::from_str(&text)?;
            Ok(parsed.device_id)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let id = uuid::Uuid::new_v4().to_string();
            let file = DeviceFile { device_id: id.clone() };
            let text = serde_json::to_string_pretty(&file)?;
            tokio::fs::write(&path, text).await?;
            Ok(id)
        }
        Err(e) => Err(e.into()),
    }
}
