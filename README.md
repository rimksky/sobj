# sobj — Simple Object Storage (v0.2)

`sobj` は **S3風のAPIを持つ、超シンプルなオブジェクトストレージ**です。  
Rust（axum）で実装されたサーバと、Rust製CLIクライアントで構成されます。

v0.2 の主な追加:
- ✅ 設定を **JSONファイル**から読み込み（標準: 実行ファイルと同じディレクトリ）
- ✅ サーバログに **接続元IP/ポート**と **in-flight数（簡易的な接続状況）** を表示
- ✅ CLI の put/get に **進捗表示**
- ✅ サーバは **Nginx無しでも外部から listen**（default: 0.0.0.0:8080）

---

## 設定ファイル

### server: `sobj-server.json`
（`sobj-server` 実行ファイルと同じディレクトリがデフォルト）

```json
{
  "listen_addr": "0.0.0.0:8080",
  "storage_dir": "./data",
  "auth_token": "Bearer devtoken"
}
```

### cli: `sobj.json`
（`sobj` 実行ファイルと同じディレクトリがデフォルト）

```json
{
  "endpoint": "http://127.0.0.1:8080",
  "token": "Bearer devtoken",
  "timeout_secs": 300
}
```

> どちらも `--config /path/to/file.json` で場所を上書きできます。

---

## ビルド

```bash
cargo build --release
```

生成物:
- `./target/release/sobj-server`
- `./target/release/sobj`

---

## 起動（ローカル）

```bash
mkdir -p ./data
./target/release/sobj-server
```

---

## CLI例

```bash
echo "hello" > hello.txt
./target/release/sobj put hello.txt foo/hello.txt
./target/release/sobj ls --prefix foo/ --delimiter /
./target/release/sobj get foo/hello.txt out.txt
./target/release/sobj head foo/hello.txt
./target/release/sobj rm foo/hello.txt
```

---

## Nginx リバースプロキシ（任意）

`nginx-sobj.conf` を参照。重要なのは Authorization を upstream に渡すこと:

```nginx
proxy_set_header Authorization $http_authorization;
```
