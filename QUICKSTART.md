# sobj quickstart (macOS) v0.2

## 1) Build
```bash
cargo build --release
```

## 2) Create config files
（デフォルトは「実行ファイルと同じディレクトリ」を見にいきます）

### server config
```bash
cat > ./target/release/sobj-server.json <<'JSON'
{
  "listen_addr": "0.0.0.0:8080",
  "storage_dir": "./data",
  "auth_token": "Bearer devtoken"
}
JSON
```

### cli config
```bash
cat > ./target/release/sobj.json <<'JSON'
{
  "endpoint": "http://127.0.0.1:8080",
  "token": "Bearer devtoken",
  "timeout_secs": 300
}
JSON
```

## 3) Run server
```bash
mkdir -p ./data
./target/release/sobj-server
```

## 4) Use CLI
```bash
echo "hello" > hello.txt
./target/release/sobj put hello.txt foo/hello.txt
./target/release/sobj ls --prefix foo/ --delimiter /
./target/release/sobj get foo/hello.txt out.txt
./target/release/sobj head foo/hello.txt
./target/release/sobj rm foo/hello.txt
```

## Override config path
```bash
./target/release/sobj-server --config ./sobj-server.json
./target/release/sobj --config ./sobj.json ls
```

## External access (no nginx)
- `listen_addr` を `0.0.0.0:8080` にすると外部から到達可能
- macOS のファイアウォールで許可が必要な場合があります
