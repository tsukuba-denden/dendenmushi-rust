#!/bin/bash

# Botの手動再起動用スクリプト
# 日常的なメンテナンス用
#
# 使用方法:
#   ./manual_restart.sh         # 通常の再起動（クリーンビルド有り）
#   ./manual_restart.sh --no-clean  # 高速再起動（クリーンビルド無し）

echo "=== Bot手動再起動スクリプト ==="
echo "実行日時: $(date '+%Y-%m-%d %H:%M:%S')"

# オプションをチェック
RESTART_OPTIONS=""
if [ "$1" = "--no-clean" ]; then
    echo "高速再起動モード（クリーンビルドをスキップ）"
    RESTART_OPTIONS="--no-clean"
else
    echo "通常再起動モード（クリーンビルド実行）"
fi

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

# 自動再起動スクリプトを実行（オプション付き）
/home/yuubinnkyoku/dendenmushi/restart_bot.sh $RESTART_OPTIONS

echo ""
echo "再起動処理が完了しました。"
echo "ログを確認するには以下のコマンドを使用してください："
echo "  tail -f /home/yuubinnkyoku/dendenmushi/logs/restart.log"
echo "  tail -f /home/yuubinnkyoku/dendenmushi/logs/bot.log"
