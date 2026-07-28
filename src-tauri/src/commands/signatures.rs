use crate::error::{AppError, AppResult};
use crate::services::lineage_manager;
use crate::services::signature_manager;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SignatureConfig {
    pub id: String,
    pub label: String,
    pub keystore_path: String,
    pub keystore_password: String,
    pub key_alias: String,
    pub key_password: String,
    pub created_at: String,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewKeystoreDName {
    #[serde(default)]
    pub cn: String,
    #[serde(default)]
    pub ou: String,
    #[serde(default)]
    pub o: String,
    #[serde(default)]
    pub l: String,
    #[serde(default)]
    pub st: String,
    #[serde(default)]
    pub c: String,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewKeystoreOptions {
    #[serde(default)]
    pub key_algorithm: String,
    #[serde(default)]
    pub key_size: u32,
    #[serde(default)]
    pub validity_days: u32,
    #[serde(default)]
    pub dname: NewKeystoreDName,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NewKeystoreInput {
    pub label: String,
    pub alias: String,
    pub keystore_password: String,
    pub key_password: String,
    #[serde(default)]
    pub options: NewKeystoreOptions,
}

#[tauri::command]
pub async fn list_signatures(app: AppHandle) -> AppResult<Vec<SignatureConfig>> {
    signature_manager::read_all(&app).await
}

#[tauri::command]
pub async fn create_signature(
    app: AppHandle,
    input: SignatureConfig,
) -> AppResult<SignatureConfig> {
    signature_manager::create(&app, input).await
}

#[tauri::command]
pub async fn update_signature(
    app: AppHandle,
    id: String,
    patch: SignatureConfig,
) -> AppResult<SignatureConfig> {
    signature_manager::update(&app, &id, patch).await
}

#[tauri::command]
pub async fn delete_signature(app: AppHandle, id: String) -> AppResult<()> {
    let referenced = lineage_manager::list_referencing_signature(&app, &id).await?;
    if !referenced.is_empty() {
        let names: Vec<String> = referenced.into_iter().map(|l| l.label).collect();
        return Err(AppError::InvalidInput(format!(
            "签名被以下 Lineage 引用,请先删除或解除关联: {}",
            names.join(", ")
        )));
    }
    signature_manager::delete(&app, &id).await
}

#[tauri::command]
pub async fn import_keystore(
    app: AppHandle,
    src_path: String,
    alias: String,
    password: String,
    label: String,
) -> AppResult<SignatureConfig> {
    signature_manager::import(&app, &PathBuf::from(src_path), alias, password, label).await
}

#[tauri::command]
pub async fn create_new_keystore(
    app: AppHandle,
    input: NewKeystoreInput,
) -> AppResult<SignatureConfig> {
    let opts = &input.options;
    let options = signature_manager::KeystoreGenOptions {
        key_algorithm: if opts.key_algorithm.is_empty() {
            "RSA".into()
        } else {
            opts.key_algorithm.clone()
        },
        key_size: if opts.key_size == 0 { 2048 } else { opts.key_size },
        validity_days: if opts.validity_days == 0 { 10950 } else { opts.validity_days },
        dname: signature_manager::DNameParts {
            cn: opts.dname.cn.clone(),
            ou: opts.dname.ou.clone(),
            o: opts.dname.o.clone(),
            l: opts.dname.l.clone(),
            st: opts.dname.st.clone(),
            c: opts.dname.c.clone(),
        },
    };
    signature_manager::create_new(
        &app,
        input.label,
        input.alias,
        input.keystore_password,
        input.key_password,
        options,
    )
    .await
}

#[tauri::command]
pub async fn export_signature(
    app: AppHandle,
    id: String,
    dest_path: String,
) -> AppResult<String> {
    let dest = PathBuf::from(&dest_path);
    signature_manager::export_keystore(&app, &id, &dest).await
}
