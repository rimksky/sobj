# sobj API Specification (v0.2)

## 概要

`sobj` は **S3 風だが極限まで簡略化したオブジェクトストレージ API** です。

- bucket 概念なし
- Host 名に依存しない
- 認証は共通トークンのみ
- フォルダはキーのプレフィックスとして扱う
- HTTP/JSON ベース
- PUT / GET はストリーミング対応（大容量ファイル可）

---

## 共通仕様

### Base URL

```
http(s)://<host>:<port>/
```

### 認証

すべての API は `Authorization` ヘッダが必須。

```
Authorization: <auth_token>
```

例:

```
Authorization: Bearer devtoken
```

不一致の場合:

```
401 Unauthorized
```

---

## オブジェクトキー（key）

- UTF-8
- `/` を含めてよい（仮想フォルダ）
- **先頭 `/` は不可**
- `..` を含むものは不可
- URL パスでは URL エンコードされた状態で送信する

### 例

OK:
```
foo/bar.txt
images/2025/a.png
```

NG:
```
/foo/bar.txt
../secret.txt
```

---

## API 一覧

| 操作 | Method | Path |
|---|---|---|
| オブジェクト作成 / 上書き | PUT | /{key} |
| オブジェクト取得 | GET | /{key} |
| オブジェクト削除 | DELETE | /{key} |
| オブジェクト情報取得 | HEAD | /{key} |
| オブジェクト一覧 | GET | / |

---

## PUT /{key}

オブジェクトをアップロードする。

### Request

```
PUT /foo/bar.txt HTTP/1.1
Authorization: Bearer devtoken
Content-Type: application/octet-stream
Content-Length: 12345

<binary data>
```

- Body はストリーミング可
- 既存オブジェクトは上書き

### Response（成功）

```
201 Created
Content-Type: application/json
```

```json
{
  "key": "foo/bar.txt",
  "size": 12345
}
```

### エラー

| Status | 内容 |
|---|---|
| 400 | InvalidKey |
| 401 | Unauthorized |
| 500 | InternalError |

---

## GET /{key}

オブジェクトをダウンロードする。

### Request

```
GET /foo/bar.txt
Authorization: Bearer devtoken
```

### Response（成功）

```
200 OK
Content-Type: <推測された MIME>
Content-Length: <bytes>
```

- Body はストリーミング
- 数 GB クラスのファイルも対応

### エラー

| Status | 内容 |
|---|---|
| 404 | NotFound |
| 401 | Unauthorized |
| 500 | InternalError |

---

## HEAD /{key}

オブジェクトのメタ情報を取得する。

### Request

```
HEAD /foo/bar.txt
Authorization: Bearer devtoken
```

### Response（成功）

```
200 OK
Content-Length: 12345
```

- Body は返らない

---

## DELETE /{key}

オブジェクトを削除する。

### Request

```
DELETE /foo/bar.txt
Authorization: Bearer devtoken
```

### Response（成功）

```
204 No Content
```

- 存在しない場合でも 204 を返す（冪等）

---

## GET /（LIST）

オブジェクト一覧を取得する。

### Query Parameters

| 名前 | 型 | 説明 |
|---|---|---|
| prefix | string | キーの前方一致 |
| delimiter | string | 疑似フォルダ区切り（例: `/`） |
| limit | number | 最大件数（default 1000 / max 10000） |
| cursor | string | ページング用カーソル |

### Request 例

```
GET /?prefix=foo/&delimiter=/
Authorization: Bearer devtoken
```

### Response（成功）

```
200 OK
Content-Type: application/json
```

```json
{
  "prefix": "foo/",
  "delimiter": "/",
  "items": [
    {
      "key": "foo/a.txt",
      "size": 123,
      "last_modified": "2025-01-16T10:12:33Z"
    }
  ],
  "common_prefixes": [
    "foo/images/"
  ],
  "next_cursor": "foo/a.txt"
}
```

### レスポンスフィールド

| フィールド | 説明 |
|---|---|
| items | 実オブジェクト一覧 |
| common_prefixes | 仮想フォルダ一覧 |
| next_cursor | 続きがある場合のみ返る |

---

## エラーレスポンス共通形式

```json
{
  "error": "InvalidKey | Unauthorized | NotFound | InternalError",
  "message": "optional detail"
}
```

---

## 非対応（意図的に省略）

- bucket
- ACL / 権限管理
- マルチパートアップロード
- バージョニング
- Range GET
- Copy / Move
- カスタムメタデータ

---

## 設計思想

- S3 互換を目指さない
- 実装が小さく、読みやすく、改造しやすい
- CLI / curl / 任意言語から扱いやすい

---

## 将来拡張候補（v0.3+）

- Range GET
- overwrite=false
- ETag（hash）
- gzip 圧縮
- read-only / write-only トークン
