#!/bin/bash

# Botの再起動スクリプト
# 日本時間午前6時にcronで実行される

# スクリプトの場所を基準にプロジェクトディレクトリを設定
PROJECT_DIR="/home/yuubinnkyoku/dendenmushi"
LOG_FILE="/home/yuubinnkyoku/dendenmushi/logs/restart.log"
BOT_LOG_FILE="/home/yuubinnkyoku/dendenmushi/logs/bot.log"

# ログディレクトリを作成
mkdir -p "$(dirname "$LOG_FILE")"

# 日付とともにログ出力
echo "$(date '+%Y-%m-%d %H:%M:%S') - Botの再起動を開始" >> "$LOG_FILE"

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

# 最新のコードでビルド
echo "$(date '+%Y-%m-%d %H:%M:%S') - プロジェクトをビルド中..." >> "$LOG_FILE"

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
    # リリースビルドを実行
    echo "$(date '+%Y-%m-%d %H:%M:%S') - リリースビルドを開始..." >> "$LOG_FILE"
    if cargo build --release >> "$LOG_FILE" 2>&1; then
        echo "$(date '+%Y-%m-%d %H:%M:%S') - ビルド成功" >> "$LOG_FILE"
    else
        echo "$(date '+%Y-%m-%d %H:%M:%S') - リリースビルドに失敗。デバッグビルドを試行..." >> "$LOG_FILE"
        if cargo build >> "$LOG_FILE" 2>&1; then
            echo "$(date '+%Y-%m-%d %H:%M:%S') - デバッグビルド成功" >> "$LOG_FILE"
        else
            echo "$(date '+%Y-%m-%d %H:%M:%S') - エラー: ビルドに失敗しました" >> "$LOG_FILE"
            exit 1
        fi
    fi
fi

# Botをバックグラウンドで起動
echo "$(date '+%Y-%m-%d %H:%M:%S') - Botを起動中..." >> "$LOG_FILE"

# 利用可能なバイナリを決定
if [ -f "./target/release/observer" ]; then
    BINARY_PATH="./target/release/observer"
    echo "$(date '+%Y-%m-%d %H:%M:%S') - リリース版バイナリを使用" >> "$LOG_FILE"
elif [ -f "./target/debug/observer" ]; then
    BINARY_PATH="./target/debug/observer"
    echo "$(date '+%Y-%m-%d %H:%M:%S') - デバッグ版バイナリを使用" >> "$LOG_FILE"
else
    echo "$(date '+%Y-%m-%d %H:%M:%S') - エラー: 実行可能なバイナリが見つかりません" >> "$LOG_FILE"
    exit 1
fi

# nohupを使ってバックグラウンドで実行
nohup $BINARY_PATH >> "$BOT_LOG_FILE" 2>&1 &

# プロセスIDを記録
BOT_PID=$!
echo "$(date '+%Y-%m-%d %H:%M:%S') - Bot起動完了 (PID: $BOT_PID)" >> "$LOG_FILE"

# 起動確認のため少し待機
sleep 5

# プロセスが実際に動作しているか確認
if ps -p $BOT_PID > /dev/null; then
    echo "$(date '+%Y-%m-%d %H:%M:%S') - Botが正常に動作しています" >> "$LOG_FILE"
else
    echo "$(date '+%Y-%m-%d %H:%M:%S') - 警告: Botプロセスが見つかりません" >> "$LOG_FILE"
fi

echo "$(date '+%Y-%m-%d %H:%M:%S') - 再起動処理完了" >> "$LOG_FILE"
