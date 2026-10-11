//! Comment threads pinned to the canvas.

use crate::{NodeId, Vec2};
use serde::{Deserialize, Serialize};

/// Where a thread is pinned. With `node`, `offset` is relative to the node's
/// top-left so the pin follows it; otherwise `offset` is in page space.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommentAnchor {
    pub page: NodeId,
    pub node: Option<NodeId>,
    pub offset: Vec2,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    /// Display name chosen in the app's settings.
    pub author: String,
    pub text: String,
    /// RFC 3339 timestamp.
    pub created: String,
    pub edited: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommentThread {
    pub id: String,
    pub anchor: CommentAnchor,
    /// First message is the thread's opening comment.
    pub messages: Vec<Comment>,
    pub resolved: bool,
}
