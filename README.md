# Pinecone Connector

Adds Pinecone vector database connectivity as an installable connector extension.

This connector is listed in the public Irodori extension marketplace.

## Connector

- Extension ID: `irodori.pinecone`
- Engine ID: `pinecone`
- Wire: `pinecone`
- Default port: `0`
- Native ABI: `irodori.connector.native.v1`
- Driver linked: `true`

The native driver uses Pinecone REST APIs for index metadata and data-plane JSON calls.

Connector metadata lives in `connector.config.json` and `irodori.extension.json`.
The Rust code keeps native ABI exports in `src/lib.rs`, shared buffer/JSON helpers in `src/abi.rs`, and Pinecone behavior in `src/driver.rs`.

## Connection Metadata

- Endpoint modes: `cloudResource`, `customEndpoint`
- Transport modes: `customEndpoint`, `direct`, `sshTunnel`, `socks5Proxy`, `httpConnectProxy`, `proxyChain`
- TLS supported: `true`
- Custom driver options: `true`

| Auth method | Label | Secret purposes |
|---|---|---|
| `none` | No authentication | none |
| `connectionString` | Connection string / DSN | none |
| `apiKey` | API key | `token` |
| `bearerToken` | Bearer token | `token` |
| `clientCertificate` | Client certificate / mTLS | `privateKey`, `privateKeyPassphrase` |
| `customDriverOptions` | Custom driver options | `password`, `token`, `privateKey`, `privateKeyPassphrase` |

## Experience Metadata

- Domains: `vector`
- Result views: `vectorNeighbors`, `table`, `json`
- Inspired by: `Pinecone indexes`, `Pinecone namespaces`, `Pinecone metadata filters`

| Workflow | Result view | Templates |
|---|---|---|
| Similarity search | vectorNeighbors | vector-similarity |
| Filtered ANN search | vectorNeighbors | vector-filtered |
| Collection or index health | table | vector-health |

| Template | Label | Language | Result view |
|---|---|---|---|
| `vector-similarity` | Pinecone query | `json` | `vectorNeighbors` |
| `vector-filtered` | Pinecone filtered query | `json` | `vectorNeighbors` |
| `vector-health` | Pinecone index stats | `text` | `table` |

## ABI Calls

The driver handles these JSON requests today:

| Method | Response |
|---|---|
| `health` / `ping` | Connector health, engine id, ABI version, and driver link status. |
| `describe` / `capabilities` | Embedded manifest and connector config. |
| `manifest` | Raw `irodori.extension.json`. |
| `config` | Raw `connector.config.json`. |
| `connect` | Opens an HTTP client and validates Pinecone index listing. |
| `query` | Sends JSON requests to the configured Pinecone data-plane endpoint. |
| `metadata` | Loads index metadata from Pinecone control-plane APIs. |
| `close` | Removes the cached native connection. |

## Development


Generated extension repositories share `../target` across sibling repositories so Rust dependencies are compiled once per checkout. DuckDB and MotherDuck are driver-linked by default; set `IRODORI_CONNECTOR_LINK_DUCKDB=0` only when you need metadata-only DuckDB-compatible scaffolds.


```sh
make check
make build
```

Release packages place platform-specific native artifacts under `dist/native`.
