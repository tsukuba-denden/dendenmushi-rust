use call_agent::function::Tool;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;

const MAX_KEYS: usize = 100;

/// メモリ操作の結果を表す構造体
#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryResponse {
    pub status: String,
    pub memory: Option<Value>,
}

/// MemoryTool 構造体：key-value ペアで記憶を管理（最大 100 件）
pub struct MemoryTool {
    memory: Mutex<HashMap<String, String>>,
}

impl MemoryTool {
    pub fn new() -> Self {
        MemoryTool {
            memory: Mutex::new(HashMap::new()),
        }
    }

    /// key-value を追加または更新する。新規キー追加時は最大数を超えるとエラー。
    pub fn add_memory(&self, key: &str, value: &str) -> Result<(), String> {
        let mut mem = self.memory.lock().map_err(|_| "Lock error".to_string())?;
        if !mem.contains_key(key) && mem.len() >= MAX_KEYS {
            return Err(format!("Cannot add new key. Maximum {} keys reached.", MAX_KEYS));
        }
        mem.insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// 指定された key の値を取得する。key が None の場合は全件返す。
    pub fn get_memory(&self, key: Option<&str>) -> HashMap<String, String> {
        let mem = self.memory.lock().unwrap();
        match key {
            Some(k) => mem.iter()
                          .filter(|(key, _)| key.as_str() == k)
                          .map(|(k, v)| (k.clone(), v.clone()))
                          .collect(),
            None => mem.clone(),
        }
    }

    /// 現在保存されているキー一覧を取得する
    pub fn get_keys(&self) -> Vec<String> {
        let mem = self.memory.lock().unwrap();
        mem.keys().cloned().collect()
    }

    /// すべてのメモリをクリアする
    pub fn clear_memory(&self) {
        let mut mem = self.memory.lock().unwrap();
        mem.clear();
    }
}

/// AI Function として利用するための Tool トレイト実装
impl Tool for MemoryTool {
    fn def_name(&self) -> &str {
        "memory_tool"
    }

    fn def_description(&self) -> &str {
        "Stores, retrieves, and clears memory as key-value pairs (maximum 100 keys). \
         Use 'add' to add/update a key-value pair, 'get' to retrieve a stored value (or all pairs if key is omitted), \
         'get_keys' to retrieve the list of current keys, and 'clear' to erase all memory."
    }

    /// JSON Schema に現在のキー一覧を反映した "key" プロパティを動的に生成する
    fn def_parameters(&self) -> serde_json::Value {
        let current_keys = self.get_keys();
        // key フィールドの schema を構築
        let key_schema = if current_keys.is_empty() {
            json!({
                "type": "string",
                "description": "Enter a new key."
            })
        } else {
            json!({
                "anyOf": [
                    {
                        "type": "string",
                        "enum": current_keys,
                        "description": "Choose an existing key from the current memory."
                    },
                    {
                        "type": "string",
                        "description": "Or enter a new key."
                    }
                ]
            })
        };

        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add", "get", "get_keys", "clear"],
                    "description": "The memory action to perform: 'add' to add/update a key-value pair, 'get' to retrieve memory, 'get_keys' to list stored keys, 'clear' to clear memory."
                },
                "key": key_schema,
                "value": {
                    "type": "string",
                    "description": "The value to store (required for 'add' action)."
                }
            },
            "required": ["action"]
        })
    }

    fn run(&self, args: serde_json::Value) -> Result<String, String> {
        let action = args.get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing or invalid 'action' parameter".to_string())?;

        match action {
            "add" => {
                let key = args.get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing or invalid 'key' parameter for add action".to_string())?;
                let value = args.get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing or invalid 'value' parameter for add action".to_string())?;
                self.add_memory(key, value)?;
                let response = MemoryResponse {
                    status: "Memory added/updated.".to_string(),
                    memory: None,
                };
                serde_json::to_string(&response).map_err(|e| e.to_string())
            },
            "get" => {
                let key = args.get("key").and_then(|v| v.as_str());
                let mem = self.get_memory(key);
                let response = MemoryResponse {
                    status: "Memory retrieved.".to_string(),
                    memory: Some(json!(mem)),
                };
                serde_json::to_string(&response).map_err(|e| e.to_string())
            },
            "get_keys" => {
                let keys = self.get_keys();
                let response = MemoryResponse {
                    status: "Key list retrieved.".to_string(),
                    memory: Some(json!(keys)),
                };
                serde_json::to_string(&response).map_err(|e| e.to_string())
            },
            "clear" => {
                self.clear_memory();
                let response = MemoryResponse {
                    status: "Memory cleared.".to_string(),
                    memory: None,
                };
                serde_json::to_string(&response).map_err(|e| e.to_string())
            },
            _ => Err("Invalid action specified. Use 'add', 'get', 'get_keys', or 'clear'.".to_string())
        }
    }
}
