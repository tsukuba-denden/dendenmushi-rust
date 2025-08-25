#!/bin/bash

# Botの再起動スクリプト（実動作テスト用）
# 実際の再起動処理を行うが、ログで詳細を確認できるようにする

PROJECT_DIR="/home/yuubinnkyoku/dendenmushi"
LOG_FILE="/home/yuubinnkyoku/dendenmushi/logs/restart_real_test.log"
BOT_LOG_FILE="/home/yuubinnkyoku/dendenmushi/logs/bot_test.log"

# ログディレクトリを作成
mkdir -p "$(dirname "$LOG_FILE")"

# 日付とともにログ出力
echo "$(date '+%Y-%m-%d %H:%M:%S') - [実動作テスト] Botの再起動を開始" >> "$LOG_FILE"

# プロジェクトディレクトリに移動
cd "$PROJECT_DIR" || {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - エラー: プロジェクトディレクトリに移動できませんでした" >> "$LOG_FILE"
    exit 1
}

# 既存のBotプロセスを終了
echo "$(date '+%Y-%m-%d %H:%M:%S') - 既存のBotプロセスを検索・終了中..." >> "$LOG_FILE"

# observer プロセスを検索して終了
pkill -f "observer" || echo "$(date '+%Y-%m-%d %H:%M:%S') - 既存のプロセスが見つかりませんでした" >> "$LOG_FILE"

# プロセスが完全に終了するまで少し待機
sleep 3

# Rustup環境をロード
export PATH="$HOME/.linuxbrew/opt/rustup/bin:$PATH"

# cargoコマンドの利用可能性をチェック
if ! command -v cargo >/dev/null 2>&1; then
    echo "$(date '+%Y-%m-%d %H:%M:%S') - 警告: cargoコマンドが利用できません。既存のバイナリを使用します" >> "$LOG_FILE"
    
    # デバッグビルドが存在するかチェック
    if [ -f "./target/debug/observer" ]; then
        echo "$(date '+%Y-%m-%d %H:%M:%S') - 既存のデバッグビルドを使用します" >> "$LOG_FILE"
    else
        echo "$(date '+%Y-%m-%d %H:%M:%S') - エラー: 利用可能なバイナリが見つかりません" >> "$LOG_FILE"
        exit 1
    fi
else
    echo "$(date '+%Y-%m-%d %H:%M:%S') - cargoコマンドが利用可能です" >> "$LOG_FILE"
    echo "$(date '+%Y-%m-%d %H:%M:%S') - [テスト] ビルドはスキップします" >> "$LOG_FILE"
fi

# 利用可能なバイナリを決定
if [ -f "./target/release/observer" ]; then
    BINARY_PATH="./target/debug/observer"  # テスト用にデバッグ版を使用
    echo "$(date '+%Y-%m-%d %H:%M:%S') - [テスト] デバッグ版バイナリを使用（テスト目的）" >> "$LOG_FILE"
elif [ -f "./target/debug/observer" ]; then
    BINARY_PATH="./target/debug/observer"
    echo "$(date '+%Y-%m-%d %H:%M:%S') - デバッグ版バイナリを使用" >> "$LOG_FILE"
else
    echo "$(date '+%Y-%m-%d %H:%M:%S') - エラー: 実行可能なバイナリが見つかりません" >> "$LOG_FILE"
    exit 1
fi

# バイナリの詳細情報をログに記録
echo "$(date '+%Y-%m-%d %H:%M:%S') - 使用するバイナリ: $BINARY_PATH" >> "$LOG_FILE"
ls -la "$BINARY_PATH" >> "$LOG_FILE" 2>&1

echo "$(date '+%Y-%m-%d %H:%M:%S') - [テスト] Botの起動はスキップします（テスト目的）" >> "$LOG_FILE"
echo "$(date '+%Y-%m-%d %H:%M:%S') - [テスト] 実際の動作では以下のコマンドが実行されます:" >> "$LOG_FILE"
echo "$(date '+%Y-%m-%d %H:%M:%S') - [テスト] nohup $BINARY_PATH >> $BOT_LOG_FILE 2>&1 &" >> "$LOG_FILE"

echo "$(date '+%Y-%m-%d %H:%M:%S') - [実動作テスト] 処理完了" >> "$LOG_FILE"
