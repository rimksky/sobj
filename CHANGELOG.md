# CHANGELOG

## v0.5.0

> ⚠️ **v0.4 系との互換性なし**
>
> 以下の破壊的変更により、v0.4 系のサーバ・CLI とは混在利用できません。
> サーバとCLI は同じバージョンを使用してください。
>
> - エンドポイント `/healthz` → `/health` に変更
> - LIST レスポンスの `common_prefixes` フィールド削除
> - LIST クエリパラメータ `delimiter` 削除
> - コピー / リネーム API（`PUT /{dst}?copy_src={src}` / `POST /rename`）削除

### Added
- CLI に `--endpoint <URL>` オプションを追加（設定ファイルの `endpoint` より優先）
- GitHub Actions によるリリースワークフロー追加（タグプッシュで全ターゲットのバイナリを自動ビルド・リリース）

### Changed
- ヘルスチェックエンドポイントを `/healthz` → `/health` に変更
- サーバコードをモジュール分割（`state` / `error` / `auth` / `key` / `handlers`）
- エラーレスポンスを `ErrorBody` で統一（`{"error":"...", "message":"..."}`）
- PUT の並行書き込み安全性向上：`AtomicU64` によるユニーク tmp ファイル名採用
- CLI のレスポンスチェックを `check_response` ヘルパーに集約
- Windows バックスラッシュのパス変換処理を削除
- LIST の `delimiter` / `common_prefixes` パラメータを削除（ファイルシステムに実フォルダがあるため不要）
- API.md を実装に合わせて全面改訂

### Removed
- オブジェクトのコピー / リネーム機能（`PUT /{dst}?copy_src={src}` / `POST /rename`）
- CLI の `cp` / `mv` サブコマンド

---

## v0.4.0

### Added
- HTTPS / TLS support for sobj-server using axum-server + rustls
- Explicit TLS enablement via `tls_enabled` and `--tls` option
- `/healthz` endpoint returning app name, version, and in-flight count
- QUICKSTART.md with HTTPS + local CA setup guide
- API.md documenting all HTTP endpoints

### Changed
- TLS certificate and key paths are resolved relative to `sobj-server.json`
- CLI CA certificate path (`tls_ca_cert_pem_path`) is resolved relative to `sobj.json`
- TLS trust model follows webpki rules (CA-signed certificates required)
- Documentation reorganized for v0.4

### Fixed
- HTTPS startup panic by explicitly selecting rustls crypto provider
- CLI TLS verification failures caused by improper trust anchor usage

### Notes
- Directly trusting a self-signed leaf certificate is **not supported**
- Use a local CA (CA:TRUE) to sign server certificates for HTTPS

---

## v0.3.1
- axum-server migration groundwork
- `/healthz` endpoint introduced
