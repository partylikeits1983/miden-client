use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// TODO - Revisit the Account struct and conform it to structure of
// the other structs in the models directory
#[wasm_bindgen]
#[derive(Serialize, Deserialize)]
#[allow(clippy::unsafe_derive_deserialize)]
pub struct SerializedAccountHeader {
    id: String,
    nonce: String,
    vault_root: String,
    storage_root: String,
    code_root: String,
}

#[wasm_bindgen]
impl SerializedAccountHeader {
    pub fn new(
        id: String,
        nonce: String,
        vault_root: String,
        storage_root: String,
        code_root: String,
    ) -> SerializedAccountHeader {
        SerializedAccountHeader {
            id,
            nonce,
            vault_root,
            storage_root,
            code_root,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn nonce(&self) -> String {
        self.nonce.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn vault_root(&self) -> String {
        self.vault_root.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn storage_root(&self) -> String {
        self.storage_root.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn code_root(&self) -> String {
        self.code_root.clone()
    }
}
