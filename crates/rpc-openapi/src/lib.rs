//! OpenAPI 3.0 spec generation for RPC methods.
//!
//! Converts RPC method schemas into OpenAPI specifications.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// OpenAPI 3.0 specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiSpec {
    pub openapi: String,
    pub info: Info,
    pub paths: HashMap<String, PathItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,
}

/// API metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    pub title: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Path item for a single endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<Operation>,
}

/// HTTP operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<RequestBody>,
    pub responses: HashMap<String, Response>,
}

/// Request body definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBody {
    pub required: bool,
    pub content: HashMap<String, MediaType>,
}

/// Media type with schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaType {
    pub schema: Value,
}

/// Response definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<HashMap<String, MediaType>>,
}

/// Reusable components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Components {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schemas: Option<HashMap<String, Value>>,
}

/// Generate OpenAPI spec from method schemas
pub fn generate_openapi_spec(
    title: &str,
    version: &str,
    method_schemas: HashMap<String, MethodSchema>,
) -> OpenApiSpec {
    let mut paths = HashMap::new();

    for (method_name, method_schema) in method_schemas {
        let path = format!("/{}", method_name);

        let request_body = if method_schema.params != Value::Null {
            Some(RequestBody {
                required: true,
                content: {
                    let mut content = HashMap::new();
                    content.insert(
                        "application/json".to_string(),
                        MediaType {
                            schema: method_schema.params,
                        },
                    );
                    content
                },
            })
        } else {
            None
        };

        let mut responses = HashMap::new();
        responses.insert(
            "200".to_string(),
            Response {
                description: "Successful response".to_string(),
                content: Some({
                    let mut content = HashMap::new();
                    content.insert(
                        "application/json".to_string(),
                        MediaType {
                            schema: method_schema.returns,
                        },
                    );
                    content
                }),
            },
        );

        paths.insert(
            path,
            PathItem {
                post: Some(Operation {
                    summary: Some(method_name.clone()),
                    description: None,
                    request_body,
                    responses,
                }),
            },
        );
    }

    OpenApiSpec {
        openapi: "3.0.0".to_string(),
        info: Info {
            title: title.to_string(),
            version: version.to_string(),
            description: None,
        },
        paths,
        components: None,
    }
}

/// Method schema information
#[derive(Debug, Clone)]
pub struct MethodSchema {
    pub name: String,
    pub params: Value,
    pub returns: Value,
}

/// Generate TypeScript client from method schemas
pub fn generate_typescript_client(
    class_name: &str,
    base_url: &str,
    method_schemas: HashMap<String, MethodSchema>,
) -> String {
    let mut methods = Vec::new();

    for (method_name, method_schema) in method_schemas {
        let ts_method = generate_ts_method(&method_name, &method_schema);
        methods.push(ts_method);
    }

    format!(
        r#"/**
 * Generated TypeScript client
 * Base URL: {}
 */

export class {} {{
  private baseUrl: string;

  constructor(baseUrl: string = "{}") {{
    this.baseUrl = baseUrl;
  }}

  private async request<T>(method: string, params?: unknown): Promise<T> {{
    const response = await fetch(`${{this.baseUrl}}/${{method}}`, {{
      method: 'POST',
      headers: {{
        'Content-Type': 'application/json',
      }},
      body: JSON.stringify(params ?? null),
    }});

    if (!response.ok) {{
      throw new Error(`RPC error: ${{response.statusText}}`);
    }}

    return response.json();
  }}

{}
}}
"#,
        base_url,
        class_name,
        base_url,
        methods.join("\n\n")
    )
}

fn generate_ts_method(method_name: &str, schema: &MethodSchema) -> String {
    let (params_type, params_usage) = schema_to_ts_params(&schema.params);
    let return_type = schema_to_ts_type(&schema.returns);

    if params_type.is_empty() {
        format!(
            r#"  async {}(): Promise<{}> {{
    return this.request<{}>('{}');
  }}"#,
            method_name, return_type, return_type, method_name
        )
    } else {
        format!(
            r#"  async {}({}): Promise<{}> {{
    return this.request<{}>('{}', {});
  }}"#,
            method_name, params_type, return_type, return_type, method_name, params_usage
        )
    }
}

fn schema_to_ts_params(schema: &Value) -> (String, String) {
    if schema.is_null() {
        return (String::new(), String::new());
    }

    // For tuple types (multiple parameters)
    if let Some(obj) = schema.as_object() {
        if obj.get("type").and_then(|v| v.as_str()) == Some("array") {
            if let Some(items) = obj.get("prefixItems").and_then(|v| v.as_array()) {
                let param_types: Vec<String> = items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let ts_type = schema_to_ts_type(item);
                        format!("arg{}: {}", i, ts_type)
                    })
                    .collect();

                let param_usage: Vec<String> =
                    (0..items.len()).map(|i| format!("arg{}", i)).collect();

                return (
                    param_types.join(", "),
                    format!("[{}]", param_usage.join(", ")),
                );
            }
        }
    }

    // Fallback for single parameter
    (
        format!("params: {}", schema_to_ts_type(schema)),
        "params".to_string(),
    )
}

fn schema_to_ts_type(schema: &Value) -> String {
    if schema.is_null() {
        return "void".to_string();
    }

    if let Some(obj) = schema.as_object() {
        if let Some(type_str) = obj.get("type").and_then(|v| v.as_str()) {
            match type_str {
                "string" => return "string".to_string(),
                "integer" | "number" => return "number".to_string(),
                "boolean" => return "boolean".to_string(),
                "array" => {
                    if let Some(items) = obj.get("items") {
                        let item_type = schema_to_ts_type(items);
                        return format!("{}[]", item_type);
                    }
                    return "unknown[]".to_string();
                }
                "object" => {
                    if let Some(props) = obj.get("properties").and_then(|v| v.as_object()) {
                        let fields: Vec<String> = props
                            .iter()
                            .map(|(k, v)| format!("{}: {}", k, schema_to_ts_type(v)))
                            .collect();
                        return format!("{{ {} }}", fields.join(", "));
                    }
                    return "Record<string, unknown>".to_string();
                }
                _ => {}
            }
        }

        // Handle enums
        if let Some(variants) = obj.get("enum").and_then(|v| v.as_array()) {
            let values: Vec<String> = variants
                .iter()
                .filter_map(|v| v.as_str().map(|s| format!("'{}'", s)))
                .collect();
            if !values.is_empty() {
                return values.join(" | ");
            }
        }
    }

    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_simple_spec() {
        let mut methods = HashMap::new();
        methods.insert(
            "add".to_string(),
            MethodSchema {
                name: "add".to_string(),
                params: serde_json::json!({
                    "type": "array",
                    "items": [
                        {"type": "integer"},
                        {"type": "integer"}
                    ]
                }),
                returns: serde_json::json!({"type": "integer"}),
            },
        );

        let spec = generate_openapi_spec("Math API", "1.0.0", methods);

        assert_eq!(spec.openapi, "3.0.0");
        assert_eq!(spec.info.title, "Math API");
        assert!(spec.paths.contains_key("/add"));
    }

    #[test]
    fn test_generate_typescript_client() {
        let mut methods = HashMap::new();
        methods.insert(
            "add".to_string(),
            MethodSchema {
                name: "add".to_string(),
                params: serde_json::json!({
                    "type": "array",
                    "prefixItems": [
                        {"type": "number"},
                        {"type": "number"}
                    ]
                }),
                returns: serde_json::json!({"type": "number"}),
            },
        );

        let client = generate_typescript_client("MathClient", "http://localhost:3000", methods);

        assert!(client.contains("class MathClient"));
        assert!(client.contains("async add("));
        assert!(client.contains("Promise<number>"));
    }
}
