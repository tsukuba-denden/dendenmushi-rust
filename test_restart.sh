#!/bin/bash

# Botの再起動スクリプト（テスト用）
# 実際のBot停止・起動は行わず、動作確認のみ

PROJECT_DIR="/home/yuubinnkyoku/dendenmushi"
LOG_FILE="/home/yuubinnkyoku/dendenmushi/logs/restart_test.log"

# ログディレクトリを作成
mkdir -p "$(dirname "$LOG_FILE")"

# 日付とともにログ出力
echo "$(date '+%Y-%m-%d %H:%M:%S') - [テスト] Botの再起動処理を開始" >> "$LOG_FILE"

# プロジェクトディレクトリに移動
cd "$PROJECT_DIR" || {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - エラー: プロジェクトディレクトリに移動できませんでした" >> "$LOG_FILE"
    exit 1
}

echo "$(date '+%Y-%m-%d %H:%M:%S') - プロジェクトディレクトリ: $(pwd)" >> "$LOG_FILE"

# Rustup環境をロード
export PATH="$HOME/.linuxbrew/opt/rustup/bin:$PATH"

# cargo コマンドが利用可能か確認
if which cargo >> "$LOG_FILE" 2>&1; then
    echo "$(date '+%Y-%m-%d %H:%M:%S') - Cargoコマンドが利用可能" >> "$LOG_FILE"
else
    echo "$(date '+%Y-%m-%d %H:%M:%S') - エラー: Cargoコマンドが見つかりません" >> "$LOG_FILE"
    exit 1
fi

# プロジェクトファイルの存在確認
if [ -f "Cargo.toml" ]; then
    echo "$(date '+%Y-%m-%d %H:%M:%S') - Cargo.tomlが見つかりました" >> "$LOG_FILE"
else
    echo "$(date '+%Y-%m-%d %H:%M:%S') - エラー: Cargo.tomlが見つかりません" >> "$LOG_FILE"
    exit 1
fi

echo "$(date '+%Y-%m-%d %H:%M:%S') - [テスト] すべてのチェックが完了しました" >> "$LOG_FILE"
echo "$(date '+%Y-%m-%d %H:%M:%S') - [テスト] 実際のビルドと再起動は実行されませんでした" >> "$LOG_FILE"
