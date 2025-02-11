use std::fmt;
use std::error::Error;

/// アプリケーションで使うエラー型
#[derive(Debug)]
pub enum ObsError {
    /// ファイルが見つからなかった場合など
    NotFound(String),
    /// 入力が不正な場合
    InvalidInput(String),
    /// I/O操作中のエラー
    IoError(std::io::Error),
    // 他のエラー型も必要に応じて追加可能
    IndexOutOfBounds,
    NotFoundPlace,
    NotFoundChannel,
    DisabledChannel,
    UnknownError,
}

impl fmt::Display for ObsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObsError::NotFound(ref msg) => write!(f, "NotFound: {}", msg),
            ObsError::InvalidInput(ref msg) => write!(f, "InvalidInput: {}", msg),
            ObsError::IoError(ref err) => write!(f, "IoError: {}", err),
            ObsError::IndexOutOfBounds => write!(f, "Index out of bounds"),
            ObsError::NotFoundPlace => write!(f, "Place not found"),
            ObsError::NotFoundChannel => write!(f, "Channel not found"),
            ObsError::DisabledChannel => write!(f, "Channel is disabled"),
            ObsError::UnknownError => write!(f, "Unknown error"),
        }
    }
}

impl Error for ObsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ObsError::IoError(ref err) => Some(err),
            _ => None,
        }
    }
}

// std::io::ErrorからAppErrorへの変換を可能にする
impl From<std::io::Error> for ObsError {
    fn from(err: std::io::Error) -> Self {
        ObsError::IoError(err)
    }
}

impl From<String> for ObsError {
    fn from(err: String) -> Self {
        ObsError::InvalidInput(err)
    }
}