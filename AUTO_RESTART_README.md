# Bot自動再起動設定

このディレクトリには、Discord Botの自動再起動機能が設定されています。

## 自動再起動について

- **実行時間**: 毎日日本時間午前6時00分
- **実行方法**: cronを使用した自動実行
- **処理内容**: 
  1. 既存のBotプロセスを安全に終了
  2. 最新のコードでリリースビルドを実行
  3. Botをバックグラウンドで再起動
  4. 実行ログの記録

## ファイル構成

- `restart_bot.sh` - 自動再起動の本体スクリプト
- `manual_restart.sh` - 手動再起動用スクリプト
- `test_restart.sh` - テスト用スクリプト
- `logs/restart.log` - 再起動処理のログ
- `logs/bot.log` - Bot実行時のログ

## 使用方法

### 自動再起動の確認
```bash
# cron設定を確認
crontab -l

# 再起動ログを確認
tail -f logs/restart.log
```

### 手動再起動
```bash
# 手動でBotを再起動
./manual_restart.sh
```

### ログの確認
```bash
# 再起動ログをリアルタイムで確認
tail -f logs/restart.log

# Botの実行ログをリアルタイムで確認
tail -f logs/bot.log
```

## cron設定

現在の設定（crontab -l で確認可能）:
```
# Bot自動再起動 - 毎日午前6時に実行
0 6 * * * /home/yuubinnkyoku/dendenmushi/restart_bot.sh
```

## トラブルシューティング

### 自動再起動が動作しない場合
1. cronサービスの状態確認: `systemctl status crond`
2. スクリプトの実行権限確認: `ls -la *.sh`
3. ログファイルの確認: `cat logs/restart.log`

### 手動でのプロセス確認
```bash
# Botプロセスの確認
ps aux | grep observer

# プロセスの手動終了（必要に応じて）
pkill -f observer
```

## 注意事項

- cronは環境変数が限定的なため、スクリプト内でPATHを設定しています
- リリースビルドに時間がかかる場合があります
- ログファイルは定期的にクリーンアップすることを推奨します

## セットアップ履歴

- 作成日: 2025-07-27
- cronサービス: crond（active/running）
- タイムゾーン: Japan (JST, +0900)
- Rust環境: ~/.linuxbrew/opt/rustup/bin/cargo
