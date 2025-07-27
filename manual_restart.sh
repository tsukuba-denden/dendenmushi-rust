#!/bin/bash

# Botの手動再起動用スクリプト
# 日常的なメンテナンス用

echo "=== Bot手動再起動スクリプト ==="
echo "実行日時: $(date '+%Y-%m-%d %H:%M:%S')"
echo ""

# 確認プロンプト
read -p "Botを再起動しますか？ (y/N): " -n 1 -r
echo    # 改行
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "キャンセルしました。"
    exit 1
fi

echo ""
echo "再起動を開始します..."

# 自動再起動スクリプトを実行
/home/yuubinnkyoku/dendenmushi/restart_bot.sh

echo ""
echo "再起動処理が完了しました。"
echo "ログを確認するには以下のコマンドを使用してください："
echo "  tail -f /home/yuubinnkyoku/dendenmushi/logs/restart.log"
echo "  tail -f /home/yuubinnkyoku/dendenmushi/logs/bot.log"
