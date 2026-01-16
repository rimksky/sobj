# sobj — Simple Object Storage (v0.3)

`sobj` は **S3風のAPIを持つ、超シンプルなオブジェクトストレージ**です。  
Rust（axum）で実装されたサーバと、Rust製CLIクライアントで構成されます。

v0.3 の追加:
- ✅ MIT License（git 公開用ファイル追加）
- ✅ Copy / Move（サーバAPI + CLIコマンド）
- ✅ (cli) ルートTLS証明書（CA）指定 / 証明書検証無効化オプション
- ✅ (cli) endpoint デフォルト `http://127.0.0.1:9999`
- ✅ (server) listen_addr デフォルト `0.0.0.0:9999`
- ✅ (server) storage_dir 相対パスの基準は `sobj-server.json` のあるディレクトリ
- ✅ (server) auth_token 省略可（空/未設定なら認証なし）
- ✅ (server) 動作確認用 `GET /healthz`

---

## Build

```bash
cargo build --release
```

生成物:
- `./target/release/sobj-server`
- `./target/release/sobj`

---

## 設定ファイル（JSON）

### server: `sobj-server.json`（デフォルト: 実行ファイルの横）

```json
{
  "listen_addr": "0.0.0.0:9999",
  "storage_dir": "./data",
  "auth_token": "Bearer devtoken"
}
```

- `auth_token` を省略または空文字 `""` にすると **認証なし**（Authorization不要）

### cli: `sobj.json`（デフォルト: 実行ファイルの横）

```json
{
  "endpoint": "http://127.0.0.1:9999",
  "token": "Bearer devtoken",
  "timeout_secs": 3600,

  "tls_ca_cert_pem_path": null,
  "tls_insecure_skip_verify": false
}
```

---

## 起動

```bash
mkdir -p ./data
./target/release/sobj-server
```

---

## CLI 例

```bash
echo "hello" > hello.txt

./target/release/sobj put hello.txt foo/hello.txt
./target/release/sobj get foo/hello.txt out.txt
./target/release/sobj ls --prefix foo/ --delimiter /

# Copy / Move
./target/release/sobj cp foo/hello.txt foo/hello-copy.txt
./target/release/sobj mv foo/hello-copy.txt foo/moved.txt

# Health check
curl http://127.0.0.1:9999/healthz
```

---

## TLS（https endpoint を使う場合）

CLI で上書き可能:

```bash
./target/release/sobj --ca-cert /path/to/ca.pem ls
./target/release/sobj --insecure get foo/bar.bin out.bin
```

JSON でも指定可能（CLIオプションが優先）:
- `tls_ca_cert_pem_path`
- `tls_insecure_skip_verify`

---

## License

MIT. See `LICENSE`.


---

## ドキュメント

- **API仕様書**: `API.md`
- **最短起動手順**: `QUICKSTART.md`
- **Nginx リバースプロキシ例（HTTP）**: `nginx-sobj.conf`
- **Nginx リバースプロキシ例（TLS/443 終端）**: `nginx-sobj-tls.conf`
- **systemd unit 例**: `sobj-server.service`

---

## 簡易仕様（要点）

### 認証
- `Authorization: <token>` ヘッダで認証する
- サーバ設定 `auth_token` が **未設定/空文字** の場合は **認証なし**

### キー（key）
- `foo/bar.txt` のように `/` を含めてよい（仮想フォルダ）
- 先頭 `/` は不可
- `..` を含むキーは不可

### エンドポイント
- **PUT `/{key}`**: アップロード（上書き）
- **GET `/{key}`**: ダウンロード（ストリーミング）
- **HEAD `/{key}`**: メタ情報取得
- **DELETE `/{key}`**: 削除（冪等）
- **GET `/`**: 一覧（prefix / delimiter / limit / cursor）
- **POST `/_copy`**: Copy（src → dst）
- **POST `/_move`**: Move（src → dst）
- **GET `/healthz`**: 動作確認（認証不要）

### 設定ファイル
- サーバ: `sobj-server.json`（実行ファイルと同じディレクトリがデフォルト）
- CLI: `sobj.json`（実行ファイルと同じディレクトリがデフォルト）

v0.3 デフォルト:
- CLI endpoint: `http://127.0.0.1:9999`
- Server listen_addr: `0.0.0.0:9999`



### Copy / Move の overwrite
- サーバAPIの `overwrite` は省略時 **true**（上書き）
- CLI はデフォルトで上書き。`--no-overwrite` を付けると上書きしない
