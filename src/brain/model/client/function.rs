use serde::{Deserialize, Serialize};

/// function call の定義  
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: serde_json::Value,
}


/// toolの定義  
pub trait Tool {
    /// 関数名  
    /// ツール名として使用される  
    fn def_name(&self) -> &str;
    /// 関数の説明  
    fn def_description(&self) -> &str;
    /// 関数のパラメータの定義(json schema)  
    fn def_parameters(&self) -> serde_json::Value;
    /// 関数の実行  
    fn run(&self, args: serde_json::Value) -> Result<String, String>;
}
