use call_agent::chat::function::Tool;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use chrono::{DateTime, Local};
use log::error;

const MEMORY_DIR: &str = "memory";
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
    /// 新しいインスタンスを生成し、memory ディレクトリ内の .md ファイルからメモリを読み込む
    pub fn new() -> Self {
        let mut mem_map = HashMap::new();

        // memory ディレクトリがなければ作成
        if let Err(e) = fs::create_dir_all(MEMORY_DIR) {
            error!("Failed to create memory directory: {}", e);
        } else {
            // memory/ 内の .md ファイルをすべて読み込む
            if let Ok(entries) = fs::read_dir(MEMORY_DIR) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension() {
                            if ext == "md" {
                                // ファイル名（拡張子除く）を key とする
                                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                    let mut file = match fs::File::open(&path) {
                                        Ok(f) => f,
                                        Err(e) => {
                                            error!("Failed to open file {:?}: {}", path, e);
                                            continue;
                                        }
                                    };
                                    let mut contents = String::new();
                                    if let Err(e) = file.read_to_string(&mut contents) {
                                        error!("Failed to read file {:?}: {}", path, e);
                                        continue;
                                    }
                                    mem_map.insert(stem.to_string(), contents);
                                }
                            }
                        }
                    }
                }
            }
        }

        MemoryTool {
            memory: Mutex::new(mem_map),
        }
    }

    /// key-value を追加または更新する。新規キー追加時は最大数を超えるとエラー。
    pub fn add_memory(&self, key: &str, value: &str) -> Result<(), String> {
        let mut mem = self.memory.lock().map_err(|_| "Lock error".to_string())?;
        if !mem.contains_key(key) && mem.len() >= MAX_KEYS {
            return Err(format!("Cannot add new key. Maximum {} keys reached.", MAX_KEYS));
        }
        mem.insert(key.to_string(), value.to_string());
        Self::save_to_file(key, value)
    }

    /// key の内容に対して新たな値を末尾に追加する (push)。既存の内容があれば改行区切りで追加、
    /// 存在しない場合は新規作成します。
    pub fn push_memory(&self, key: &str, value: &str) -> Result<(), String> {
        let mut mem = self.memory.lock().map_err(|_| "Lock error".to_string())?;
        let new_value = if let Some(existing) = mem.get(key) {
            format!("{}\n{}", existing, value)
        } else {
            // 新規の場合でも、MAX_KEYS チェックを行う
            if mem.len() >= MAX_KEYS {
                return Err(format!("Cannot add new key. Maximum {} keys reached.", MAX_KEYS));
            }
            value.to_string()
        };
        mem.insert(key.to_string(), new_value.clone());
        Self::save_to_file(key, new_value.as_str())
    }

    /// 指定された key の値を取得する。key が None の場合は全件返す。
    /// ※ run() で各エントリの最終更新日時を付与して返します。
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

    /// 指定した key のメモリをクリアする。key が None の場合は全てのメモリをクリアする
    pub fn clear_memory(&self, key: Option<&str>) {
        match key {
            Some(k) => {
                let mut mem = self.memory.lock().unwrap();
                mem.remove(k);
                let file_path = Self::get_file_path(k);
                if file_path.exists() {
                    if let Err(e) = fs::remove_file(&file_path) {
                        error!("Failed to remove file {:?}: {}", file_path, e);
                    }
                }
            }
            None => {
                let mut mem = self.memory.lock().unwrap();
                mem.clear();
                if let Ok(entries) = fs::read_dir(MEMORY_DIR) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_file() && path.extension().map(|ext| ext == "md").unwrap_or(false) {
                            if let Err(e) = fs::remove_file(&path) {
                                error!("Failed to remove file {:?}: {}", path, e);
                            }
                        }
                    }
                }
            }
        }
    }

    /// 指定した key と value を .md ファイルに保存する (上書き)。
    fn save_to_file(key: &str, value: &str) -> Result<(), String> {
        let file_path = Self::get_file_path(key);
        let mut file = fs::File::create(&file_path)
            .map_err(|e| format!("Failed to create file {:?}: {}", file_path, e))?;
        file.write_all(value.as_bytes())
            .map_err(|e| format!("Failed to write to file {:?}: {}", file_path, e))?;
        Ok(())
    }

    /// 指定した key に対応する .md ファイルのパスを取得する
    fn get_file_path(key: &str) -> PathBuf {
        let mut path = PathBuf::from(MEMORY_DIR);
        path.push(format!("{}.md", key));
        path
    }

    /// 指定した key に対応する .md ファイルの最終更新日時を取得する
    /// (人間に読みやすい形式: "YYYY-MM-DD HH:MM:SS")
    fn get_last_modified(key: &str) -> Option<String> {
        let file_path = Self::get_file_path(key);
        if let Ok(metadata) = fs::metadata(&file_path) {
            if let Ok(modified) = metadata.modified() {
                // SystemTime をローカル日時に変換
                let datetime: DateTime<Local> = modified.into();
                return Some(datetime.format("%Y-%m-%d %H:%M:%S").to_string());
            }
        }
        None
    }
}

/// AI Function として利用するための Tool トレイト実装
impl Tool for MemoryTool {
    fn def_name(&self) -> &str {
        "memory_tool"
    }

    fn def_description(&self) -> &str {
        "This tool is used to periodically save links, materials, diary entries, interesting conversations, and fascinating insights.
Each record is stored as a separate .md file in the 'memory' directory, with a maximum of 100 entries.
Use 'add' to create or update an entry, 'push' to append to an existing entry, 'get' to retrieve records (with last modified date),
'get_keys' to list all entries, and 'clear' to remove an entry (if a key is provided) or all entries."
    }

    /// JSON Schema に現在のキー一覧を反映した "key" プロパティを動的に生成する
    fn def_parameters(&self) -> serde_json::Value {
        let current_keys = self.get_keys();
        let key_schema = if current_keys.is_empty() {
            json!({
                "type": "string",
                "description": "Enter a new key (optional for 'clear' action to clear all memory)."
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
                        "description": "Or enter a new key (optional for 'clear' action to clear all memory)."
                    }
                ]
            })
        };

        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add", "push", "get", "get_keys", "clear"],
                    "description": "The memory action to perform: 'add' to add/update an entry, 'push' to append to an entry, 'get' to retrieve an entry or all entries (with last modified date), 'get_keys' to list keys, 'clear' to remove an entry (if 'key' is provided) or all entries."
                },
                "key": key_schema,
                "value": {
                    "type": "string",
                    "description": "The value to store (required for 'add' and 'push' actions)."
                },
                "$explain": {
                    "type": "string",
                    "description": "A brief explanation of what you are doing with this tool."
                },
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
            "push" => {
                let key = args.get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing or invalid 'key' parameter for push action".to_string())?;
                let value = args.get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing or invalid 'value' parameter for push action".to_string())?;
                self.push_memory(key, value)?;
                let response = MemoryResponse {
                    status: format!("Value pushed to key '{}'.", key),
                    memory: None,
                };
                serde_json::to_string(&response).map_err(|e| e.to_string())
            },
            "get" => {
                let key = args.get("key").and_then(|v| v.as_str());
                let mem = self.get_memory(key);
                let mut mem_with_meta = serde_json::Map::new();
                for (k, v) in mem.iter() {
                    mem_with_meta.insert(k.clone(), json!({
                        "value": v,
                        "last_modified": Self::get_last_modified(k)
                    }));
                }
                let response = MemoryResponse {
                    status: "Memory retrieved.".to_string(),
                    memory: Some(Value::Object(mem_with_meta)),
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
                let key = args.get("key").and_then(|v| v.as_str());
                self.clear_memory(key);
                let status_msg = match key {
                    Some(k) => format!("Memory for key '{}' cleared.", k),
                    None => "All memory cleared.".to_string(),
                };
                let response = MemoryResponse {
                    status: status_msg,
                    memory: None,
                };
                serde_json::to_string(&response).map_err(|e| e.to_string())
            },
            _ => Err("Invalid action specified. Use 'add', 'push', 'get', 'get_keys', or 'clear'.".to_string())
        }
    }
}
