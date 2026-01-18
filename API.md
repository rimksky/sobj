# API.md — sobj v0.4

このドキュメントは **sobj-server v0.4 の HTTP/HTTPS API 仕様**をまとめたものです。  
ストレージの永続化はローカルファイルシステムを前提とします。

---

## 共通仕様

### Base URL
```
http://{host}:{port}
https://{host}:{port}
```

### 認証
- `auth_token` が設定されている場合、以下の HTTP ヘッダが必須

```
Authorization: Bearer <token>
```

未指定または不正な場合は `401 Unauthorized`。

---

## ヘルスチェック

### GET /healthz

サーバの生存確認用エンドポイント。  
**認証不要**。

#### レスポンス例
```json
{
  "app": "sobj-server",
  "version": "0.4.0",
  "status": "ok",
  "in_flight": 0
}
```

#### フィールド
| フィールド | 型 | 説明 |
|---|---|---|
| app | string | アプリケーション名 |
| version | string | サーババージョン |
| status | string | `"ok"` 固定 |
| in_flight | number | 処理中リクエスト数 |

---

## オブジェクト一覧

### GET /

保存されているオブジェクトの一覧を返す。

#### レスポンス例
```json
{
  "objects": [
    {
      "key": "foo.txt",
      "size": 123,
      "mtime": "2026-01-01T12:00:00Z"
    }
  ]
}
```

---

## オブジェクト取得

### GET /{key}

指定したキーのオブジェクトを取得する。

- 存在しない場合は `404 Not Found`

#### レスポンス
- Body: オブジェクトのバイナリデータ
- `Content-Type` は拡張子から推測

---

## オブジェクトメタデータ取得

### HEAD /{key}

オブジェクト本体を返さず、メタデータのみ取得する。

#### レスポンスヘッダ
```
Content-Length: <size>
Last-Modified: <RFC3339>
```

---

## オブジェクト保存

### PUT /{key}

指定したキーでオブジェクトを保存する。

- 既存キーがある場合は **上書き**

#### リクエスト
- Body: 任意のバイナリ

#### レスポンス
```
201 Created
```

---

## オブジェクト削除

### DELETE /{key}

指定したキーのオブジェクトを削除する。

- 存在しない場合は `404 Not Found`

#### レスポンス
```
204 No Content
```

---

## オブジェクトコピー

### POST /_copy

既存オブジェクトを別キーにコピーする。

#### リクエスト
```json
{
  "from": "src.txt",
  "to": "dst.txt"
}
```

#### レスポンス例
```json
{
  "ok": true
}
```

---

## オブジェクト移動

### POST /_move

既存オブジェクトを別キーに移動（rename）する。

#### リクエスト
```json
{
  "from": "src.txt",
  "to": "dst.txt"
}
```

#### レスポンス例
```json
{
  "ok": true
}
```

---

## ステータスコード一覧

| ステータス | 説明 |
|---|---|
| 200 | OK |
| 201 | Created |
| 204 | No Content |
| 400 | Bad Request |
| 401 | Unauthorized |
| 404 | Not Found |
| 500 | Internal Server Error |

---

## 注意事項

- パスは URL デコード後にファイルパスとして扱われる
- ディレクトリ区切り（`/`）を含むキーも使用可能
- TLS / HTTPS の詳細は `README.md` / `QUICKSTART.md` を参照

---

## 互換性

- v0.4.x 系では API 互換性を維持
- 破壊的変更は v0.5 で実施予定

