# sobj — Simple Object Storage (v0.4)

`sobj` は、ローカルファイルシステムをバックエンドとする  
**軽量なオブジェクトストレージサーバ + CLI クライアント**です。

- シンプルな HTTP/HTTPS API
- nginx などのリバースプロキシ配下でも利用可能
- ローカル開発〜小規模用途向け

---

## ドキュメント

- [QUICKSTART](QUICKSTART.md) — 最短で動かす手順（HTTPS 含む）
- [API](API.md) — HTTP/HTTPS API 仕様
- [CHANGELOG](CHANGELOG.md) — バージョンごとの変更点

---

## Build

```bash
cargo build --release
```

生成物：

- `target/release/sobj-server`
- `target/release/sobj`

---

## 設定ファイル

### server: `sobj-server.json`

> ⚠️ 相対パス（`storage_dir`, `tls.cert_pem`, `tls.key_pem`）は  
> **`sobj-server.json` が置かれているディレクトリ基準**で解決されます。

### 最小構成（HTTP）

```json
{
  "listen_addr": "0.0.0.0:9999",
  "storage_dir": "./data"
}
```

### 認証付き（HTTP）

```json
{
  "listen_addr": "0.0.0.0:9999",
  "storage_dir": "./data",
  "auth_token": "Bearer devtoken"
}
```

### HTTPS 有効化例

```json
{
  "listen_addr": "0.0.0.0:9999",
  "storage_dir": "./data",
  "auth_token": "Bearer devtoken",

  "tls_enabled": true,
  "tls": {
    "cert_pem": "./tls/server.cert.pem",
    "key_pem": "./tls/server.key.pem"
  }
}
```

---

### cli: `sobj.json`

> ⚠️ 相対パス（`tls_ca_cert_pem_path`）は  
> **`sobj.json` が置かれているディレクトリ基準**で解決されます。

```json
{
  "endpoint": "https://localhost:9999",
  "token": "Bearer devtoken",
  "timeout_secs": 3600,

  "tls_ca_cert_pem_path": "./tls/ca.cert.pem",
  "tls_insecure_skip_verify": false
}
```

---

## 起動方法

### HTTP

```bash
mkdir -p ./data
./target/release/sobj-server
```

### HTTPS

```bash
./target/release/sobj-server --tls
```

---

## API（概要）

### ヘルスチェック

```http
GET /healthz
```

- 認証不要
- サーバの生存確認用
- 詳細は [API.md](API.md) を参照

---

## 運用例

### nginx 配下で利用する場合

```json
{
  "listen_addr": "127.0.0.1:9999",
  "storage_dir": "/var/lib/sobj"
}
```

nginx 側で HTTPS 終端：

```nginx
proxy_pass http://127.0.0.1:9999;
```

---

## License

MIT

