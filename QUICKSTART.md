# QUICKSTART — sobj v0.4

このドキュメントは **sobj v0.4 を最短で動かす手順**です。  
HTTPS（TLS）込みで、**5〜10分で疎通確認**できることを目標にしています。

---

## 前提

- OS: macOS / Linux
- `openssl` が利用可能
- `cargo build --release` 済み
  - `target/release/sobj-server`
  - `target/release/sobj`

---

## 1. ディレクトリ構成（例）

```text
sobj/
├─ target/release/
│  ├─ sobj-server
│  └─ sobj
├─ tls/
│  ├─ ca.cert.pem
│  ├─ ca.key.pem
│  ├─ server.cert.pem
│  └─ server.key.pem
├─ data/
├─ sobj-server.json
└─ sobj.json
```

---

## 2. ローカル CA を作る（1回だけ）

```bash
mkdir -p tls
cd tls
```

### CA 秘密鍵
```bash
openssl genrsa -out ca.key.pem 2048
```

### CA 証明書（CA:TRUE）
```bash
openssl req -x509 -new -nodes   -key ca.key.pem   -sha256   -days 3650   -subj "/CN=sobj-dev-ca"   -addext "basicConstraints=critical,CA:TRUE"   -addext "keyUsage=critical,keyCertSign,cRLSign"   -out ca.cert.pem
```

確認：
```bash
openssl x509 -in ca.cert.pem -noout -text | grep -n "Basic Constraints" -A2
```

---

## 3. サーバ証明書を作る（SAN 付き）

### サーバ秘密鍵
```bash
openssl genrsa -out server.key.pem 2048
```

### CSR 作成
```bash
openssl req -new   -key server.key.pem   -subj "/CN=localhost"   -out server.csr.pem
```

### 拡張定義（SAN）
```bash
cat > server.ext <<'EOF'
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectAltName=DNS:localhost,IP:127.0.0.1
EOF
```

### CA で署名
```bash
openssl x509 -req   -in server.csr.pem   -CA ca.cert.pem   -CAkey ca.key.pem   -CAcreateserial   -out server.cert.pem   -days 365   -sha256   -extfile server.ext
```

確認：
```bash
openssl x509 -in server.cert.pem -noout -text | grep -n "Subject Alternative Name" -A2
```

---

## 4. sobj-server の設定

`sobj-server.json`

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

> 相対パスは **`sobj-server.json` の場所基準**です。

---

## 5. sobj-server を起動

```bash
mkdir -p data
./target/release/sobj-server --tls
```

ログ例：
```text
sobj-server listening on https://0.0.0.0:9999
```

---

## 6. CLI の設定

`sobj.json`

```json
{
  "endpoint": "https://localhost:9999",
  "token": "Bearer devtoken",
  "timeout_secs": 3600,
  "tls_ca_cert_pem_path": "./tls/ca.cert.pem",
  "tls_insecure_skip_verify": false
}
```

> 相対パスは **`sobj.json` の場所基準**です。

---

## 7. 動作確認

### curl
```bash
curl --cacert ./tls/ca.cert.pem https://localhost:9999/healthz
```

### CLI
```bash
./target/release/sobj ls
```

成功すれば空の一覧が返ります。

---

## 8. よくあるエラー

### ❌ certificate was not trusted
- CA ではなく server.cert.pem を指定している
- `BasicConstraints: CA:TRUE` が無い CA を使っている

### ❌ SAN エラー
- 証明書に `DNS:localhost` が無い
- `https://127.0.0.1` でアクセスしている

---

## 次に読む

- `README.md` — 全体仕様
- `API.md` — API 詳細

