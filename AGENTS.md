# 運用手順書: 低RAMビルド＆実行

リソースが限られたコンテナ内でビルドなどを実行するため、ビルドとリンク時のRAM使用量を極限まで削減することを目標としています。

## 前提条件 (最小構成のUbuntu)
最小限のツールチェインとOpenSSL開発ライブラリをインストールします（vendoredビルドではなく、システムへのリンク）。

```bash
sudo apt-get update
sudo apt-get install -y libssl-dev pkg-config clang lld libc6-dev
```

- `clang` + `lld` はリンカのメモリ使用量を低く抑えます。
- `libssl-dev` は、vendoredビルドを回避しつつ、間接的に`native-tls`/OpenSSLを必要とするcrateの依存関係を満たします。

## リポジトリのデフォルト設定
低RAM運用のためのデフォルト設定が `.cargo/config.toml` に記述されています:

- `jobs = 1`
- `incremental = false` (開発・リリース共通)
- `codegen-units = 1`, `lto = "off"` (リリースビルド時)
- リンカ: `x86_64-unknown-linux-gnu` では `clang` と `-fuse-ld=lld` を使用

ホストがx86_64でない場合、ターゲットセクションのキーを適宜修正してください。以下で確認できます:

```bash
rustc -vV | grep host
```

## 実行方法 (低RAM)
推奨コマンド（リリースビルド、時間はかかりますがRAM使用量は最小）:

```bash
CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 cargo run --release
```

より高速な開発ビルド（こちらも低RAM）:

```bash
CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 cargo run
```

実行時に追加の制約を強制する（リポジトリのデフォルト設定と重複しますが、CIなどで有用）:

```bash
CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUSTFLAGS="-C codegen-units=1 -C lto=off" cargo run --release
```

## 補足
- 直接の `openssl` 依存を削除するとコンパイル時のRAMは減りますが、一部の間接的なcrateが `native-tls` 経由でOpenSSLを要求する場合があります。システムの `libssl-dev` を使うことで、vendoredなCコードのビルドを回避できます。
- `cc not found` でリンクに失敗する場合、`clang lld libc6-dev` がインストールされているか確認してください。
- どのcrateがOpenSSLを依存に加えているか調べるには:

```bash
cargo tree -i openssl-sys -e features
```
