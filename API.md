# API.md — sobj v0.4

**sobj-server v0.4 の HTTP/HTTPS API 仕様**。
ストレージの永続化はローカルファイルシステムを前提とします。

---

## 共通仕様

### Base URL
```
http://{host}:{port}
https://{host}:{port}
```

### 認証

設定ファイルで `auth_token` を指定している場合、全エンドポイント（`/health` を除く）で以下のヘッダが必須。

```
Authorization: <token>
```

`token` は設定ファイルの `auth_token` の値をそのまま使用する。
ヘッダが未指定または値が一致しない場合は `401 Unauthorized`。

### エラーレスポンス形式

エラー時は常に以下の JSON を返す。`message` は省略される場合がある。

```json
{
  "error": "ErrorCode",
  "message": "詳細メッセージ"
}
```

| `error` の値 | ステータス | 説明 |
|---|---|---|
| `Unauthorized` | 401 | 認証ヘッダが未指定または不一致 |
| `InvalidKey` | 400 | キーが空、`/` で始まる、または `..` を含む |
| `NotFound` | 404 | 指定したキーが存在しない |
| `InternalError` | 500 | サーバ内部エラー |

### キーの制約

- 空文字は不可
- `/` で始まるキーは不可
- `..` を含むキーは不可（パストラバーサル防止）
- `/` をパス区切りとして使用可能（例: `images/2024/photo.jpg`）

---

## ヘルスチェック

### GET /health

サーバの生存確認用エンドポイント。**認証不要**。

#### レスポンス `200 OK`

```json
{
  "app": "sobj-server",
  "version": "0.4.2",
  "status": "ok",
  "in_flight": 3
}
```

| フィールド | 型 | 説明 |
|---|---|---|
| `app` | string | アプリケーション名（固定値 `"sobj-server"`） |
| `version` | string | サーババージョン |
| `status` | string | 固定値 `"ok"` |
| `in_flight` | number | 現在処理中のリクエスト数 |

---

## オブジェクト一覧

### GET /

保存されているオブジェクトの一覧を返す。

#### クエリパラメータ

| パラメータ | 型 | デフォルト | 説明 |
|---|---|---|---|
| `prefix` | string | `""` | このプレフィックスで始まるキーのみ返す |
| `limit` | number | `1000` | 返すアイテムの最大件数（上限 `10000`） |
| `cursor` | string | `""` | ページネーション用カーソル。前回レスポンスの `next_cursor` を指定する |

#### レスポンス `200 OK`

```json
{
  "prefix": "images/",
  "items": [
    {
      "key": "images/photo.jpg",
      "size": 204800,
      "last_modified": "2026-03-01T12:00:00Z"
    }
  ],
  "next_cursor": "images/photo.jpg"
}
```

| フィールド | 型 | 説明 |
|---|---|---|
| `prefix` | string | リクエスト時の `prefix`（未指定時は `""`） |
| `items` | array | マッチしたオブジェクトの配列（キー昇順） |
| `items[].key` | string | オブジェクトのキー |
| `items[].size` | number | バイト数 |
| `items[].last_modified` | string \| null | 最終更新日時（RFC 3339）。取得できない場合は `null` |
| `next_cursor` | string \| null | 次ページが存在する場合に返すカーソル値。`null` の場合は最終ページ |

#### ページネーション

`next_cursor` が `null` でない場合、次のリクエストで `cursor=<next_cursor>` を指定すると続きを取得できる。

```
GET /?limit=100&cursor=images/photo.jpg
```

---

## オブジェクト保存

### PUT /{key}

指定したキーでオブジェクトを保存する。既存キーがある場合は**上書き**。

#### リクエスト

- ボディ: 任意のバイナリ
- `Content-Type`: 任意（レスポンスには影響しない）

#### レスポンス `201 Created`

```json
{
  "key": "images/photo.jpg",
  "size": 204800
}
```

| フィールド | 型 | 説明 |
|---|---|---|
| `key` | string | 保存されたオブジェクトのキー |
| `size` | number | 保存されたバイト数 |

---

## オブジェクト取得

### GET /{key}

指定したキーのオブジェクトをストリーミングで返す。

#### レスポンス `200 OK`

- ボディ: オブジェクトのバイナリデータ
- `Content-Type`: キーの拡張子から推測（不明な場合は `application/octet-stream`）
- `Content-Length`: バイト数

存在しない場合は `404 NotFound`。

---

## オブジェクトメタデータ取得

### HEAD /{key}

オブジェクト本体を返さず、サイズのみ確認する。

#### レスポンス `200 OK`

ボディなし。以下のレスポンスヘッダを返す。

| ヘッダ | 説明 |
|---|---|
| `Content-Length` | オブジェクトのバイト数 |

存在しない場合は `404 NotFound`。

---

## オブジェクト削除

### DELETE /{key}

指定したキーのオブジェクトを削除する。
キーが存在しない場合でも `204 No Content` を返す（冪等）。

#### レスポンス `204 No Content`

ボディなし。

---

## ステータスコード一覧

| ステータス | 説明 |
|---|---|
| 200 | OK |
| 201 | Created（PUT 成功） |
| 204 | No Content（DELETE 成功） |
| 400 | Bad Request（不正なキー） |
| 401 | Unauthorized（認証失敗） |
| 404 | Not Found |
| 500 | Internal Server Error |

---

## 注意事項

- アップロード中の一時ファイル（`*.uploading.tmp`）は一覧に表示されない
- TLS / HTTPS の設定については `QUICKSTART.md` を参照
