# QUICKSTART (sobj v0.3)

このドキュメントは **macOS (ローカル)** で sobj を最短で動かす手順です。

---

## 1. ビルド

```bash
git clone <your repo url>
cd sobj
cargo build --release
```

生成物:
- `./target/release/sobj-server`
- `./target/release/sobj`

---

## 2. 設定ファイル作成（実行ファイルの横）

`sobj` は **設定JSONを「実行ファイルと同じディレクトリ」から自動ロード**します。

### server: sobj-server.json

```bash
cat > ./target/release/sobj-server.json <<'JSON'
{
  "listen_addr": "0.0.0.0:9999",
  "storage_dir": "./data",
  "auth_token": "Bearer devtoken"
}
JSON
```

### cli: sobj.json

```bash
cat > ./target/release/sobj.json <<'JSON'
{
  "endpoint": "http://127.0.0.1:9999",
  "token": "Bearer devtoken",
  "timeout_secs": 3600
}
JSON
```

---

## 3. サーバ起動

```bash
mkdir -p ./data
./target/release/sobj-server
```

---

## 4. 動作確認（healthz）

```bash
curl http://127.0.0.1:9999/healthz
```

---

## 5. CLI 動作確認

```bash
echo hello > hello.txt

./target/release/sobj put hello.txt test/hello.txt
./target/release/sobj ls --prefix test/ --delimiter /
./target/release/sobj get test/hello.txt out.txt

./target/release/sobj cp test/hello.txt test/hello-copy.txt
./target/release/sobj mv test/hello-copy.txt test/hello-moved.txt
```

---

## 6. TLS を使う場合（任意）

### 6.1 自前 CA を追加する（推奨）

```bash
./target/release/sobj --ca-cert /path/to/ca.pem ls
```

### 6.2 TLS 検証を無効化する（非推奨）

```bash
./target/release/sobj --insecure ls
```
