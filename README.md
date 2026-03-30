# sobj — Simple Object Storage (v0.5)

`sobj` は、ローカルファイルシステムをバックエンドとする
**軽量なオブジェクトストレージサーバ + CLI クライアント**です。

- シンプルな HTTP/HTTPS API
- キーをそのままファイルシステムパスにマップ（`images/photo.jpg` → `./data/images/photo.jpg`）
- nginx などのリバースプロキシ配下でも利用可能
- ローカル開発〜小規模用途向け

---

## ドキュメント

- [QUICKSTART](QUICKSTART.md) — 最短で動かす手順（HTTPS 含む）
- [API](API.md) — HTTP/HTTPS API 仕様
- [CHANGELOG](CHANGELOG.md) — バージョンごとの変更点

---

## ダウンロード

GitHub Releases からビルド済みバイナリをダウンロードできます。

| プラットフォーム | ファイル名 |
|---|---|
| Windows x64 | `sobj-server-*-windows-x64.tar.gz` / `sobj-*-windows-x64.tar.gz` |
| Linux x64（静的リンク） | `sobj-server-*-linux-x64.tar.gz` / `sobj-*-linux-x64.tar.gz` |
| Raspberry Pi 4/5, Zero 2W | `sobj-server-*-linux-aarch64.tar.gz` / `sobj-*-linux-aarch64.tar.gz` |
| Raspberry Pi 3/2（32bit OS） | `sobj-server-*-linux-armv7.tar.gz` / `sobj-*-linux-armv7.tar.gz` |
| Raspberry Pi Zero/Zero W | `sobj-server-*-linux-arm.tar.gz` / `sobj-*-linux-arm.tar.gz` |

---

## Build

ソースからビルドする場合：

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

#### 最小構成（HTTP）

```json
{
  "listen_addr": "0.0.0.0:9999",
  "storage_dir": "./data"
}
```

#### 認証付き（HTTP）

```json
{
  "listen_addr": "0.0.0.0:9999",
  "storage_dir": "./data",
  "auth_token": "Bearer devtoken"
}
```

#### HTTPS 有効化例

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

設定ファイルが存在しない場合は組み込みデフォルト値が使用されます（endpoint: `http://127.0.0.1:9999`、認証なし）。

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

## CLI コマンド

| コマンド | 説明 |
|---|---|
| `sobj put <local> <key>` | ファイルをアップロード |
| `sobj get <key> <local>` | ファイルをダウンロード |
| `sobj ls` | オブジェクト一覧を表示 |
| `sobj head <key>` | オブジェクトの存在・サイズを確認 |
| `sobj rm <key>` | オブジェクトを削除 |
| `sobj health` | サーバの生存確認 |

### グローバルオプション

| オプション | 説明 |
|---|---|
| `--endpoint <URL>` | 接続先 URL（設定ファイルより優先） |
| `--config <path>` | 設定ファイルパスを明示指定 |
| `--ca-cert <path>` | ルート CA 証明書 PEM（設定ファイルより優先） |
| `--insecure` | TLS 証明書検証をスキップ（開発用） |

```bash
# 設定ファイルなしで直接接続先を指定
sobj --endpoint http://192.168.1.10:9999 ls

# HTTPS + CA 証明書指定
sobj --endpoint https://myserver.local:9443 --ca-cert ./tls/ca.cert.pem health

# 設定ファイルを明示指定
sobj --config /path/to/sobj.json ls

# ls のオプション
sobj ls --prefix images/ --limit 100 --json

# put の Content-Type を明示指定
sobj put ./photo.jpg images/photo.jpg --content-type image/jpeg
```

---

## API（概要）

| メソッド | パス | 説明 |
|---|---|---|
| `GET` | `/health` | ヘルスチェック（認証不要） |
| `GET` | `/` | オブジェクト一覧 |
| `PUT` | `/{key}` | オブジェクト保存（上書き） |
| `GET` | `/{key}` | オブジェクト取得 |
| `HEAD` | `/{key}` | メタデータ取得（サイズのみ） |
| `DELETE` | `/{key}` | オブジェクト削除（冪等） |

詳細は [API.md](API.md) を参照。

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
