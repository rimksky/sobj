# sobj API Specification (v0.3)

## 概要

`sobj` は **S3 風だが極限まで簡略化したオブジェクトストレージ API** です。

- bucket 概念なし
- Host 名に依存しない
- 認証は共通トークン（ただし v0.3 で省略可）
- フォルダはキーのプレフィックスとして扱う
- HTTP/JSON ベース
- PUT / GET はストリーミング対応（大容量ファイル可）

---

## 共通仕様

### Base URL

```
http(s)://<host>:<port>/
```

### 認証（任意）

`auth_token` が **未設定 / 空文字** の場合、認証は無効で `Authorization` は不要。

認証が有効な場合は、すべての API で `Authorization` ヘッダ必須。

```
Authorization: <auth_token>
```

---

## オブジェクトキー（key）

- UTF-8
- `/` を含めてよい（仮想フォルダ）
- **先頭 `/` は不可**
- `..` を含むものは不可
- URL パスでは URL エンコードされた状態で送信する

---

## API 一覧

| 操作 | Method | Path |
|---|---|---|
| オブジェクト作成 / 上書き | PUT | /{key} |
| オブジェクト取得 | GET | /{key} |
| オブジェクト削除 | DELETE | /{key} |
| オブジェクト情報取得 | HEAD | /{key} |
| オブジェクト一覧 | GET | / |
| Copy | POST | /_copy |
| Move | POST | /_move |
| Health check | GET | /healthz |

---

## Copy / Move

### POST /_copy

Request:

```json
{ "src": "foo/a.bin", "dst": "bar/a.bin", "overwrite": true }
```

Response:

```json
{ "src": "foo/a.bin", "dst": "bar/a.bin", "size": 12345 }
```

### POST /_move

Request:

```json
{ "src": "foo/a.bin", "dst": "bar/a.bin", "overwrite": true }
```

Response:

```json
{ "src": "foo/a.bin", "dst": "bar/a.bin", "size": 12345 }
```

---

## GET /healthz

認証不要。

```json
{ "status": "ok" }
```
