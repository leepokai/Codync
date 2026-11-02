//! The graph a workflow saves, and the enums on the wire.
//!
//! Unit variants are a bare camelCase string. A schedule reuses `routines::Trigger`,
//! which is already tagged with `type`.

use crate::routines::Trigger;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeKind {
    Trigger,
    Prompt,
    Agent,
    Condition,
    Approval,
    Output,
    Text,
    Note,
    Shape,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trigger => "trigger",
            Self::Prompt => "prompt",
            Self::Agent => "agent",
            Self::Condition => "condition",
            Self::Approval => "approval",
            Self::Output => "output",
            Self::Text => "text",
            Self::Note => "note",
            Self::Shape => "shape",
        }
    }

    pub fn is_annotation(self) -> bool {
        matches!(self, Self::Text | Self::Note | Self::Shape)
    }

    pub fn is_step(self) -> bool {
        !self.is_annotation()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PortType {
    Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Branch {
    Yes,
    No,
}

impl Branch {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TriggerKind {
    Manual,
    Schedule,
    Webhook,
}

impl TriggerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Schedule => "schedule",
            Self::Webhook => "webhook",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "manual" => Some(Self::Manual),
            "schedule" => Some(Self::Schedule),
            "webhook" => Some(Self::Webhook),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputTarget {
    Notify,
    Chat,
}

impl OutputTarget {
    pub fn parse(raw: Option<&str>) -> Self {
        if raw == Some("chat") { Self::Chat } else { Self::Notify }
    }

    pub fn is_chat(self) -> bool {
        matches!(self, Self::Chat)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    Running,
    Approval,
    Stopping,
    Done,
    Failed,
    Stopped,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Approval => "approval",
            Self::Stopping => "stopping",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "running" => Some(Self::Running),
            "approval" => Some(Self::Approval),
            "stopping" => Some(Self::Stopping),
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            "stopped" => Some(Self::Stopped),
            _ => None,
        }
    }

    pub fn is_open(self) -> bool {
        matches!(self, Self::Running | Self::Approval | Self::Stopping)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepStatus {
    Running,
    Approval,
    Done,
    Failed,
    Stopped,
    Skipped,
}

impl StepStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Approval => "approval",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Skipped => "skipped",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "running" => Some(Self::Running),
            "approval" => Some(Self::Approval),
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            "stopped" => Some(Self::Stopped),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }

    pub fn is_open(self) -> bool {
        matches!(self, Self::Running | Self::Approval)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunSource {
    Manual,
    Schedule,
    Webhook,
}

impl RunSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Schedule => "schedule",
            Self::Webhook => "webhook",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VersionKind {
    Auto,
    Restore,
    Run,
}

impl VersionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Restore => "restore",
            Self::Run => "run",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub at: Point,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<Size>,
    #[serde(default)]
    pub locked: bool,
    pub fields: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<Trigger>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub port: PortType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<Branch>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub id: String,
    pub brush: String,
    pub color: String,
    pub width: f64,
    pub points: Vec<Point>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub strokes: Vec<Stroke>,
}

/// What a node accepts and what it emits. Annotations have neither.
pub fn ports(kind: NodeKind) -> (&'static [PortType], &'static [PortType]) {
    match kind {
        NodeKind::Trigger | NodeKind::Prompt => (&[], &[PortType::Text]),
        NodeKind::Agent | NodeKind::Condition | NodeKind::Approval => (&[PortType::Text], &[PortType::Text]),
        NodeKind::Output => (&[PortType::Text], &[]),
        NodeKind::Text | NodeKind::Note | NodeKind::Shape => (&[], &[]),
    }
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// A new workflow's graph: one manual trigger, ready to run.
pub fn default_graph() -> Graph {
    let mut fields = Map::new();
    fields.insert("trigger".into(), Value::String("manual".into()));
    Graph {
        nodes: vec![Node {
            id: new_id(),
            kind: NodeKind::Trigger,
            at: Point { x: 80.0, y: 80.0 },
            size: None,
            locked: false,
            fields,
            schedule: None,
        }],
        edges: Vec::new(),
        strokes: Vec::new(),
    }
}

pub fn field_str<'a>(fields: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    fields.get(key).and_then(Value::as_str)
}
