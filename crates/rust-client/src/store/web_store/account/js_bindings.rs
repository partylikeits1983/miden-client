use alloc::{string::String, vec::Vec};

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{js_sys, wasm_bindgen};

// Account IndexedDB Operations
#[wasm_bindgen(module = "/src/store/web_store/js/accounts.js")]
extern "C" {
    // GETS
    // ================================================================================================
    #[wasm_bindgen(js_name = getAccountIds)]
    pub fn idxdb_get_account_ids() -> js_sys::Promise;

    #[wasm_bindgen(js_name = getAllAccountHeaders)]
    pub fn idxdb_get_account_headers() -> js_sys::Promise;

    #[wasm_bindgen(js_name = getAccountHeader)]
    pub fn idxdb_get_account_header(account_id: String) -> js_sys::Promise;

    #[wasm_bindgen(js_name = getAccountHeaderByHash)]
    pub fn idxdb_get_account_header_by_hash(account_hash: String) -> js_sys::Promise;

    #[wasm_bindgen(js_name = getAccountCode)]
    pub fn idxdb_get_account_code(code_root: String) -> js_sys::Promise;

    #[wasm_bindgen(js_name = getAccountStorage)]
    pub fn idxdb_get_account_storage(storage_root: String) -> js_sys::Promise;

    #[wasm_bindgen(js_name = getAccountAssetVault)]
    pub fn idxdb_get_account_asset_vault(vault_root: String) -> js_sys::Promise;

    // INSERTS
    // ================================================================================================

    #[wasm_bindgen(js_name = insertAccountCode)]
    pub fn idxdb_insert_account_code(code_root: String, code: Vec<u8>) -> js_sys::Promise;

    #[wasm_bindgen(js_name = insertAccountStorage)]
    pub fn idxdb_insert_account_storage(
        storage_root: String,
        storage_slots: Vec<u8>,
    ) -> js_sys::Promise;

    #[wasm_bindgen(js_name = insertAccountAssetVault)]
    pub fn idxdb_insert_account_asset_vault(vault_root: String, assets: Vec<u8>)
        -> js_sys::Promise;

    #[wasm_bindgen(js_name = insertAccountRecord)]
    pub fn idxdb_insert_account_record(
        id: String,
        code_root: String,
        storage_root: String,
        vault_root: String,
        nonce: String,
        committed: bool,
        account_seed: Option<Vec<u8>>,
        hash: String,
    ) -> js_sys::Promise;

    #[wasm_bindgen(js_name = upsertForeignAccountCode)]
    pub fn idxdb_upsert_foreign_account_code(
        account_id: String,
        code: Vec<u8>,
        code_root: String,
    ) -> js_sys::Promise;

    #[wasm_bindgen(js_name = getForeignAccountCode)]
    pub fn idxdb_get_foreign_account_code(account_ids: Vec<String>) -> js_sys::Promise;

    // UPDATES
    // ================================================================================================

    #[wasm_bindgen(js_name = lockAccount)]
    pub fn idxdb_lock_account(account_id: String) -> js_sys::Promise;
}
