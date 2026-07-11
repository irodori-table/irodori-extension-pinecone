<!-- i18n: language-switcher -->
[English](README.md) | [日本語](README.ja.md)

# Pinecone コネクタ

Pinecone 用のネイティブ Irodori Table コネクタ拡張です。

このクレートは、Irodori 拡張マーケットプレイスで使用されるコネクタのメタデータ、ネイティブ ABI エクスポート、およびドライバー実装をパッケージ化しています。

## コネクタ

- 拡張 ID: `irodori.pinecone`
- エンジン ID: `pinecone`
- ワイヤープロトコル: `pinecone`
- デフォルトポート: `0`
- ネイティブ ABI: `irodori.connector.native.v1`
- ドライバー連携: `yes`
- マーケットプレイス公開範囲: `public`
- パッケージバージョン: `0.1.3`

このパッケージはコネクタのメタデータとネイティブドライバーを直接使用しており、デスクトップアダプターのソーススナップショットは不要です。

コネクタメタデータは `connector.config.json` と `irodori.extension.json` にあります。
Rust クレートは `src/lib.rs` からネイティブ ABI をエクスポートし、共有 JSON/バッファヘルパーに `irodori-connector-abi` を使用し、コネクタの動作は `src/driver.rs` に保持しています。

## 接続メタデータ

- エンドポイントモード: `cloudResource`, `customEndpoint`
- トランスポートモード: `customEndpoint`, `direct`, `sshTunnel`, `socks5Proxy`, `httpConnectProxy`, `proxyChain`
- TLS 対応: `yes`
- デフォルトで TLS 必須: `no`
- カスタムドライバーオプション: `yes`

### エンドポイントフィールド

| フィールド | ラベル | 型 | 必須 |
| --- | --- | --- | --- |
| `environment` | Pinecone 環境またはリージョン | `string` | いいえ |
| `indexHost` | インデックスホスト | `uri` | いいえ |

## 認証

コネクタはこれらの認証モードを宣伝しており、クライアントは適切な認証情報フィールドを表示できます。
ドライバー固有またはプロバイダー固有の値は、必要に応じて `options` 経由で渡すことも可能です。

| 認証方法 | ラベル | 種類 | 秘密の用途 |
| --- | --- | --- | --- |
| `apiKey` | Pinecone API キー | `apiKey` | `token` |
| `customDriverOptions` | カスタムドライバーオプション | `custom` | `password`, `token`, `privateKey`, `privateKeyPassphrase` |

## エクスペリエンスメタデータ

- ドメイン: `vector`
- 結果ビュー: `vectorNeighbors`, `table`, `json`
- オブジェクトタイプ: `collections`, `indexes`, `vectors`, `payloadFields`, `partitions`, `namespaces`
- インスパイア元: Pinecone インデックス、Pinecone ネームスペース、Pinecone メタデータフィルター

| ワークフロー | 結果ビュー | テンプレート |
| --- | --- | --- |
| 類似検索 | `vectorNeighbors` | `vector-similarity` |
| フィルタ付き ANN 検索 | `vectorNeighbors` | `vector-filtered` |
| コレクションまたはインデックスのヘルス | `table` | `vector-health` |

| テンプレート | ラベル | 言語 | 結果ビュー |
| --- | --- | --- | --- |
| `vector-similarity` | Pinecone クエリ | `json` | `vectorNeighbors` |
| `vector-filtered` | Pinecone フィルタ付きクエリ | `json` | `vectorNeighbors` |
| `vector-health` | Pinecone インデックス統計 | `text` | `table` |

## ネイティブ ABI コール

| メソッド | レスポンス |
| --- | --- |
| `health` | コネクタのヘルス、エンジン ID、ABI バージョン、ドライバー状態を返します。 |
| `describe` | 埋め込みマニフェストとコネクタ設定を返します。 |
| `manifest` | 生の `irodori.extension.json` を返します。 |
| `config` | 生の `connector.config.json` を返します。 |
| `connect` | ネイティブコネクタ接続を開き、検証します。 |
| `query` | コネクタクエリを実行し、構造化された行または JSON 結果を返します。 |
| `metadata` | スキーマ、テーブル、カラム、インデックス、コレクション、または同等のメタデータを読み取ります。 |
| `close` | キャッシュされたネイティブ接続を閉じて削除します。 |

## 開発

このチェックアウト内のすべての拡張クレートは `../target` を共有しており、依存関係は兄弟リポジトリ間で一度だけコンパイルされます。

```sh
make check
make build
```

リリースパッケージはプラットフォーム固有のネイティブアーティファクトを `dist/native` に配置します。

## ライセンス

0BSD。ほぼあらゆる目的でこのプロジェクトを使用、コピー、修正、配布できます。