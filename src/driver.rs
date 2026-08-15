use irodori_connector_abi::{collect_url_auth, option_string, push_sensitive};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use reqwest::{Client, RequestBuilder};
use serde_json::{json, Map, Value};
use tokio::runtime::Runtime;

use crate::abi::{self, IrodoriConnectorBuffer};
use crate::{ABI_VERSION, CONFIG_JSON, DRIVER_LINKED, ENGINE, MANIFEST_JSON};

static CONNECTIONS: OnceLock<Mutex<HashMap<String, PineconeConnection>>> = OnceLock::new();
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

#[derive(Clone)]
struct PineconeConnection {
    client: Client,
    config: PineconeConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PineconeConfig {
    base_url: String,
    api_key: Option<String>,
    bearer_token: Option<String>,
    redaction_values: Vec<String>,
}

type QueryOutput = (Vec<String>, Vec<Vec<Value>>, bool);

fn connections() -> &'static Mutex<HashMap<String, PineconeConnection>> {
    CONNECTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime() -> Result<&'static Runtime, String> {
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let runtime = Runtime::new().map_err(|err| format!("create tokio runtime failed: {err}"))?;
    let _ = RUNTIME.set(runtime);
    RUNTIME
        .get()
        .ok_or_else(|| "create tokio runtime failed.".to_string())
}

pub fn call_json(request: IrodoriConnectorBuffer) -> IrodoriConnectorBuffer {
    let request = match abi::parse_request(request) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let method = match abi::request_method(request.as_ref()) {
        Ok(method) => method,
        Err(response) => return response,
    };

    match method {
        "health" | "ping" => abi::ok(Map::from_iter([
            ("engine".to_string(), Value::String(ENGINE.to_string())),
            ("abiVersion".to_string(), json!(ABI_VERSION)),
            ("driverLinked".to_string(), Value::Bool(DRIVER_LINKED)),
        ])),
        "describe" | "capabilities" => abi::ok(Map::from_iter([
            ("engine".to_string(), Value::String(ENGINE.to_string())),
            ("abiVersion".to_string(), json!(ABI_VERSION)),
            ("driverLinked".to_string(), Value::Bool(DRIVER_LINKED)),
            (
                "manifest".to_string(),
                serde_json::from_str(MANIFEST_JSON).unwrap_or(Value::Null),
            ),
            (
                "config".to_string(),
                serde_json::from_str(CONFIG_JSON).unwrap_or(Value::Null),
            ),
        ])),
        "manifest" => abi::owned_buffer(MANIFEST_JSON.to_string()),
        "config" => abi::owned_buffer(CONFIG_JSON.to_string()),
        "connect" => connect(request.as_ref().expect("connect has request")),
        "query" => query(request.as_ref().expect("query has request")),
        "metadata" => metadata(request.as_ref().expect("metadata has request")),
        "close" => close(request.as_ref().expect("close has request")),
        other => abi::error(
            "connector.unknownMethod",
            format!("unknown connector method: {other}"),
        ),
    }
}

fn connect(request: &Value) -> IrodoriConnectorBuffer {
    let connection_id = abi::connection_id(Some(request));
    let config = match PineconeConfig::from_request(request) {
        Ok(config) => config,
        Err(err) => return abi::error("connector.invalidRequest", err),
    };
    let connection = PineconeConnection {
        client: Client::new(),
        config,
    };
    let index_count = match runtime().and_then(|runtime| runtime.block_on(probe(&connection))) {
        Ok(index_count) => index_count,
        Err(err) => return abi::error("connector.connectFailed", connection.config.redact(&err)),
    };
    let mut guard = match connections().lock() {
        Ok(guard) => guard,
        Err(_) => {
            return abi::error(
                "connector.statePoisoned",
                "Connector connection state is poisoned.",
            )
        }
    };
    let response = Map::from_iter([
        ("engine".to_string(), Value::String(ENGINE.to_string())),
        (
            "connectionId".to_string(),
            Value::String(connection_id.clone()),
        ),
        ("driverLinked".to_string(), Value::Bool(DRIVER_LINKED)),
        (
            "endpoint".to_string(),
            Value::String(connection.config.base_url.clone()),
        ),
        ("indexCount".to_string(), json!(index_count)),
    ]);
    guard.insert(connection_id, connection);
    abi::ok(response)
}

fn query(request: &Value) -> IrodoriConnectorBuffer {
    let connection_id = abi::connection_id(Some(request));
    let Some(input) = abi::string_field(request, "query")
        .or_else(|| abi::string_field(request, "sql"))
        .or_else(|| abi::string_field(request, "statement"))
    else {
        return abi::error(
            "connector.invalidRequest",
            "query requires a JSON query string for the Pinecone data-plane API.",
        );
    };
    let body: Value = serde_json::from_str(input)
        .unwrap_or_else(|_| json!({ "query": input, "topK": abi::max_rows(request) }));
    let path = body
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("/query")
        .to_string();
    let payload = body.get("body").cloned().unwrap_or(body);
    let connection = match connection(&connection_id) {
        Ok(connection) => connection,
        Err(response) => return response,
    };
    match runtime()
        .and_then(|runtime| runtime.block_on(run_data_request(&connection, &path, payload)))
    {
        Ok((columns, rows, truncated)) => abi::ok(Map::from_iter([
            ("connectionId".to_string(), Value::String(connection_id)),
            (
                "columns".to_string(),
                Value::Array(columns.into_iter().map(Value::String).collect()),
            ),
            (
                "rows".to_string(),
                Value::Array(rows.into_iter().map(Value::Array).collect()),
            ),
            ("truncated".to_string(), Value::Bool(truncated)),
        ])),
        Err(err) => abi::error("connector.queryFailed", connection.config.redact(&err)),
    }
}

fn metadata(request: &Value) -> IrodoriConnectorBuffer {
    let connection_id = abi::connection_id(Some(request));
    let connection = match connection(&connection_id) {
        Ok(connection) => connection,
        Err(response) => return response,
    };
    match runtime().and_then(|runtime| runtime.block_on(load_metadata(&connection))) {
        Ok(metadata) => abi::ok(Map::from_iter([
            ("connectionId".to_string(), Value::String(connection_id)),
            ("metadata".to_string(), metadata),
        ])),
        Err(err) => abi::error("connector.metadataFailed", connection.config.redact(&err)),
    }
}

fn close(request: &Value) -> IrodoriConnectorBuffer {
    let connection_id = abi::connection_id(Some(request));
    let mut guard = match connections().lock() {
        Ok(guard) => guard,
        Err(_) => {
            return abi::error(
                "connector.statePoisoned",
                "Connector connection state is poisoned.",
            )
        }
    };
    let existed = guard.remove(&connection_id).is_some();
    abi::ok(Map::from_iter([
        ("connectionId".to_string(), Value::String(connection_id)),
        ("closed".to_string(), Value::Bool(existed)),
    ]))
}

impl PineconeConnection {
    fn auth(&self, builder: RequestBuilder) -> RequestBuilder {
        if let Some(api_key) = self.config.api_key.as_deref() {
            builder.header("Api-Key", api_key)
        } else if let Some(token) = self.config.bearer_token.as_deref() {
            builder.bearer_auth(token)
        } else {
            builder
        }
    }
}

impl PineconeConfig {
    fn from_request(request: &Value) -> Result<Self, String> {
        let base_url = option_string(request, &["connectionString", "url", "dsn", "endpoint"])
            .unwrap_or_else(|| "https://api.pinecone.io".to_string());
        // The desktop form labels the password field "API key / token" for this
        // engine, so a key typed there arrives as `password`. Resolve it in a
        // second pass rather than appending to the list above: `option_string`
        // scans container-first, and `password` sits in the profile container
        // while an explicit `apiKey` usually sits in `options` — one combined
        // list would let a stale password shadow the explicit option.
        let api_key = option_string(request, &["apiKey", "api_key"])
            .or_else(|| option_string(request, &["password"]));
        let bearer_token = option_string(request, &["token", "bearerToken", "accessToken"]);
        let mut redaction_values = Vec::new();
        push_sensitive(&mut redaction_values, api_key.as_deref());
        push_sensitive(&mut redaction_values, bearer_token.as_deref());
        collect_url_auth(&base_url, &mut redaction_values);
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            bearer_token,
            redaction_values,
        })
    }

    fn redact(&self, message: &str) -> String {
        self.redaction_values.iter().fold(
            message.replace(&self.base_url, "<pinecone-url>"),
            |message, secret| {
                if secret.is_empty() {
                    message
                } else {
                    message.replace(secret, "****")
                }
            },
        )
    }
}

async fn probe(connection: &PineconeConnection) -> Result<usize, String> {
    let value = control_get(connection, "/indexes").await?;
    Ok(indexes_from_response(&value).len())
}

async fn load_metadata(connection: &PineconeConnection) -> Result<Value, String> {
    let value = control_get(connection, "/indexes").await?;
    let objects = indexes_from_response(&value)
        .into_iter()
        .map(|index| {
            let name = index.get("name").and_then(Value::as_str).unwrap_or("");
            json!({
                "schema": "default",
                "name": name,
                "kind": "index",
                "columns": [
                    {"name": "id", "dataType": "vector_id", "nullable": false, "ordinal": 1},
                    {"name": "score", "dataType": "float", "nullable": true, "ordinal": 2},
                    {"name": "metadata", "dataType": "json", "nullable": true, "ordinal": 3}
                ],
                "indexes": [],
                "primaryKey": [{"name": "id", "keyType": "primary"}],
                "foreignKeys": [],
                "details": index
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({ "schemas": [{ "name": "default", "objects": objects }] }))
}

async fn control_get(connection: &PineconeConnection, path: &str) -> Result<Value, String> {
    let response = connection
        .auth(
            connection
                .client
                .get(format!("{}{}", connection.config.base_url, path)),
        )
        .header("X-Pinecone-API-Version", "2025-01")
        .send()
        .await
        .map_err(|err| format!("Pinecone request failed: {err}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|err| format!("Pinecone response read failed: {err}"))?;
    if !status.is_success() {
        return Err(format!("Pinecone returned HTTP {status}: {text}"));
    }
    serde_json::from_str::<Value>(&text)
        .map_err(|err| format!("Pinecone JSON parse failed: {err}: {text}"))
}

async fn run_data_request(
    connection: &PineconeConnection,
    path: &str,
    body: Value,
) -> Result<QueryOutput, String> {
    let response = connection
        .auth(connection.client.post(format!(
            "{}{}",
            connection.config.base_url,
            if path.starts_with('/') {
                path.to_string()
            } else {
                format!("/{path}")
            }
        )))
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("Pinecone data-plane request failed: {err}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|err| format!("Pinecone data-plane response read failed: {err}"))?;
    if !status.is_success() {
        return Err(format!(
            "Pinecone data-plane returned HTTP {status}: {text}"
        ));
    }
    let value = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "value": text }));
    Ok(pinecone_response_to_output(value))
}

fn indexes_from_response(value: &Value) -> Vec<Value> {
    value
        .get("indexes")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| value.as_array().cloned())
        .unwrap_or_default()
}

fn pinecone_response_to_output(value: Value) -> QueryOutput {
    let matches = value
        .get("matches")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| vec![value]);
    let rows = matches
        .into_iter()
        .map(|item| {
            vec![
                item.get("id").cloned().unwrap_or(Value::Null),
                item.get("score").cloned().unwrap_or(Value::Null),
                item.get("metadata").cloned().unwrap_or(item),
            ]
        })
        .collect::<Vec<_>>();
    (
        vec![
            "id".to_string(),
            "score".to_string(),
            "metadata".to_string(),
        ],
        rows,
        false,
    )
}

fn connection(connection_id: &str) -> Result<PineconeConnection, IrodoriConnectorBuffer> {
    let guard = connections().lock().map_err(|_| {
        abi::error(
            "connector.statePoisoned",
            "Connector connection state is poisoned.",
        )
    })?;
    guard.get(connection_id).cloned().ok_or_else(|| {
        abi::error(
            "connector.connectionNotFound",
            format!("no open connection: {connection_id}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_indexes() {
        let indexes = indexes_from_response(&json!({"indexes": [{"name": "docs"}]}));
        assert_eq!(indexes[0]["name"], "docs");
    }

    #[test]
    fn maps_query_matches() {
        let (columns, rows, truncated) = pinecone_response_to_output(json!({
            "matches": [{"id": "a", "score": 0.9, "metadata": {"title": "A"}}]
        }));
        assert_eq!(columns, vec!["id", "score", "metadata"]);
        assert_eq!(rows[0][0], json!("a"));
        assert!(!truncated);
    }

    #[test]
    fn builds_config() {
        let config = PineconeConfig::from_request(&json!({
            "profile": {
                "endpoint": "https://index-host.pinecone.io",
                "apiKey": "secret"
            }
        }))
        .unwrap();
        assert_eq!(config.base_url, "https://index-host.pinecone.io");
        assert_eq!(config.api_key.as_deref(), Some("secret"));
    }

    #[test]
    fn takes_the_api_key_from_the_password_field() {
        // The connection form labels `password` "API key / token" for pinecone,
        // so this is the shape a profile filled in through the UI arrives as.
        let config = PineconeConfig::from_request(&json!({
            "profile": {
                "endpoint": "https://index-host.pinecone.io",
                "password": "pcsk_from_the_form"
            }
        }))
        .unwrap();
        assert_eq!(config.api_key.as_deref(), Some("pcsk_from_the_form"));
    }

    #[test]
    fn explicit_api_key_option_wins_over_password() {
        let config = PineconeConfig::from_request(&json!({
            "profile": {
                "password": "stale",
                "options": { "apiKey": "explicit" }
            }
        }))
        .unwrap();
        assert_eq!(config.api_key.as_deref(), Some("explicit"));
    }

    #[test]
    fn redacts_an_api_key_taken_from_the_password_field() {
        let config = PineconeConfig::from_request(&json!({
            "profile": { "password": "pcsk_secret" }
        }))
        .unwrap();
        assert_eq!(
            config.redact("rejected key pcsk_secret"),
            "rejected key ****"
        );
    }
}
