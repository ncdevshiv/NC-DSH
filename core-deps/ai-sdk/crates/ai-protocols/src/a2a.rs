//! A2A (Agent-to-Agent) protocol: agent cards, tasks, and messages over a
//! JSON endpoint (client + server).

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use ai_errors::{AiError, WebError};

/// An agent's public card (A2A agent card).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<Skill>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// A skill an agent advertises (name + description).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// Role of a message in a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Agent,
}

/// A message within a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    #[serde(default)]
    pub parts: Vec<MessagePart>,
}

/// A message part (text or file).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum MessagePart {
    Text {
        text: String,
    },
    File {
        name: String,
        mime_type: String,
        bytes: String,
    },
}

/// A task artifact (e.g. a generated file or answer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<serde_json::Value>,
}

/// Task lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Canceled,
    Failed,
}

/// A task managed by an A2A agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<Message>,
}

/// Handler implemented by A2A agents.
#[async_trait]
pub trait A2AAgent: Send + Sync {
    fn agent_card(&self) -> AgentCard;
    /// Handles an incoming task message; returns the resulting task.
    async fn handle_message(&self, task_id: &str, message: Message) -> Result<Task, AiError>;
}

/// Handler type for closure-based agents.
pub type AgentHandler = Box<dyn Fn(&str, Message) -> Result<Task, AiError> + Send + Sync>;

/// A closure-based agent for tests and simple deployments.
pub struct FunctionAgent {
    card: AgentCard,
    handler: AgentHandler,
}

impl FunctionAgent {
    pub fn new(
        card: AgentCard,
        handler: impl Fn(&str, Message) -> Result<Task, AiError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            card,
            handler: Box::new(handler),
        }
    }
}

#[async_trait]
impl A2AAgent for FunctionAgent {
    fn agent_card(&self) -> AgentCard {
        self.card.clone()
    }

    async fn handle_message(&self, task_id: &str, message: Message) -> Result<Task, AiError> {
        (self.handler)(task_id, message)
    }
}

/// The A2A server: handles JSON-RPC style method calls and serves the agent
/// card.
pub struct A2AServer {
    agent: Arc<dyn A2AAgent>,
}

impl A2AServer {
    pub fn new(agent: Arc<dyn A2AAgent>) -> Self {
        Self { agent }
    }

    pub fn agent_card(&self) -> AgentCard {
        self.agent.agent_card()
    }

    /// Handles one A2A JSON request body.
    pub async fn handle_json(&self, body: &str) -> Result<String, AiError> {
        let request: crate::jsonrpc::JsonRpcRequest = serde_json::from_str(body).map_err(|e| {
            AiError::Serialization(ai_errors::SerializationError::new(e.to_string()))
        })?;
        let id = request.id.clone();
        let response = match request.method.as_str() {
            "agentCard" => crate::jsonrpc::JsonRpcResponse::ok(
                id,
                serde_json::to_value(self.agent_card()).map_err(|e| {
                    AiError::Serialization(ai_errors::SerializationError::new(e.to_string()))
                })?,
            ),
            "message/send" => {
                let task_id = request
                    .params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("task-1")
                    .to_string();
                let message: Message = match request.params.get("message").cloned() {
                    Some(m) => serde_json::from_value(m).map_err(|e| {
                        AiError::Serialization(ai_errors::SerializationError::new(e.to_string()))
                    })?,
                    None => Message {
                        role: MessageRole::User,
                        parts: vec![],
                    },
                };
                match self.agent.handle_message(&task_id, message).await {
                    Ok(task) => {
                        let task_value = serde_json::to_value(task).map_err(|e| {
                            AiError::Serialization(ai_errors::SerializationError::new(
                                e.to_string(),
                            ))
                        })?;
                        crate::jsonrpc::JsonRpcResponse::ok(
                            id,
                            serde_json::json!({"result": task_value}),
                        )
                    }
                    Err(e) => crate::jsonrpc::JsonRpcResponse::err(id, -32603, e.to_string()),
                }
            }
            _ => crate::jsonrpc::JsonRpcResponse::err(
                id,
                -32601,
                format!("method not found: {}", request.method),
            ),
        };
        serde_json::to_string(&response)
            .map_err(|e| AiError::Serialization(ai_errors::SerializationError::new(e.to_string())))
    }
}

/// An A2A client speaking to a remote agent's JSON endpoint.
#[derive(Clone)]
pub struct A2AClient {
    endpoint: String,
    client: reqwest::Client,
}

impl A2AClient {
    pub fn new(endpoint: impl Into<String>) -> Result<Self, AiError> {
        Ok(Self {
            endpoint: endpoint.into(),
            client: reqwest::Client::builder()
                .user_agent("ai-sdk-a2a/0.1")
                .build()
                .map_err(|e| AiError::Web(WebError::new("a2a client", e.to_string())))?,
        })
    }

    async fn post(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        let request = crate::jsonrpc::JsonRpcRequest::new(serde_json::json!(1), method, params);
        let response = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|e| AiError::Web(WebError::new("a2a", e.to_string())))?;
        if !response.status().is_success() {
            return Err(AiError::Web(WebError::new(
                "a2a",
                format!("HTTP {}", response.status()),
            )));
        }
        let json: crate::jsonrpc::JsonRpcResponse = response
            .json()
            .await
            .map_err(|e| AiError::Web(WebError::new("a2a parse", e.to_string())))?;
        if let Some(error) = json.error {
            return Err(AiError::Web(WebError::new("a2a", error.to_string())));
        }
        json.result
            .ok_or_else(|| AiError::Web(WebError::new("a2a", "response without result")))
    }

    pub async fn get_agent_card(&self) -> Result<AgentCard, AiError> {
        let result = self.post("agentCard", serde_json::json!({})).await?;
        serde_json::from_value(result)
            .map_err(|e| AiError::Serialization(ai_errors::SerializationError::new(e.to_string())))
    }

    pub async fn send_message(&self, task_id: &str, message: Message) -> Result<Task, AiError> {
        let result = self
            .post(
                "message/send",
                serde_json::json!({"id": task_id, "message": message}),
            )
            .await?;
        let task_value = result.get("result").cloned().unwrap_or(result);
        serde_json::from_value(task_value)
            .map_err(|e| AiError::Serialization(ai_errors::SerializationError::new(e.to_string())))
    }
}

/// Advertised skills of an agent (helper).
pub fn skill(
    id: impl Into<String>,
    name: impl Into<String>,
    description: impl Into<String>,
) -> Skill {
    Skill {
        id: id.into(),
        name: name.into(),
        description: description.into(),
    }
}

/// Extra metadata for tasks.
pub type TaskMetadata = BTreeMap<String, serde_json::Value>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonrpc::JsonRpcRequest;

    fn echo_agent() -> Arc<dyn A2AAgent> {
        Arc::new(FunctionAgent::new(
            AgentCard {
                name: "echo-agent".into(),
                description: "echoes messages".into(),
                url: "http://localhost:9000/".into(),
                skills: vec![skill("echo", "Echo", "Echoes the input text")],
                version: Some("1.0.0".into()),
            },
            |task_id, message| {
                let text = message
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        MessagePart::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(Task {
                    id: task_id.to_string(),
                    status: TaskStatus::Completed,
                    artifacts: vec![Artifact {
                        name: "echo".into(),
                        artifact: Some(serde_json::json!({"text": text})),
                    }],
                    status_message: None,
                })
            },
        ))
    }

    #[tokio::test]
    async fn a2a_server_handles_card_and_messages() {
        let server = A2AServer::new(echo_agent());
        let card_json = server
            .handle_json(r#"{"jsonrpc":"2.0","id":1,"method":"agentCard","params":{}}"#)
            .await
            .unwrap();
        let response: crate::jsonrpc::JsonRpcResponse = serde_json::from_str(&card_json).unwrap();
        let card: AgentCard = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(card.name, "echo-agent");
        assert_eq!(card.skills.len(), 1);

        let request = JsonRpcRequest::new(
            serde_json::json!(2),
            "message/send",
            serde_json::json!({
                "id": "t-1",
                "message": {"role": "user", "parts": [{"kind": "text", "text": "hello a2a"}]}
            }),
        );
        let json = server
            .handle_json(&serde_json::to_string(&request).unwrap())
            .await
            .unwrap();
        let response: crate::jsonrpc::JsonRpcResponse = serde_json::from_str(&json).unwrap();
        let task: Task =
            serde_json::from_value(response.result.unwrap()["result"].clone()).unwrap();
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(
            task.artifacts[0].artifact.as_ref().unwrap()["text"],
            "hello a2a"
        );

        // Unknown method → JSON-RPC error.
        let json = server
            .handle_json(r#"{"jsonrpc":"2.0","id":3,"method":"nope","params":{}}"#)
            .await
            .unwrap();
        let response: crate::jsonrpc::JsonRpcResponse = serde_json::from_str(&json).unwrap();
        assert!(response.error.is_some());
    }

    #[tokio::test]
    async fn a2a_client_server_over_tcp() {
        // A real A2A exchange: raw HTTP server (tokio) + A2AClient.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let server = A2AServer::new(echo_agent());

        let server_task = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 8192];
                let n = socket.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let body = request.split("\r\n\r\n").nth(1).unwrap_or("{}");
                let response_body = server.handle_json(body).await.unwrap();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });

        let client = A2AClient::new(format!("http://{addr}/")).unwrap();
        let card = client.get_agent_card().await.unwrap();
        assert_eq!(card.name, "echo-agent");

        let task = client
            .send_message(
                "tcp-task",
                Message {
                    role: MessageRole::User,
                    parts: vec![MessagePart::Text {
                        text: "over tcp".into(),
                    }],
                },
            )
            .await
            .unwrap();
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(
            task.artifacts[0].artifact.as_ref().unwrap()["text"],
            "over tcp"
        );

        server_task.abort();
    }
}
