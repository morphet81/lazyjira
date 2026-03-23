use serde_json::Value;

/// Render an Atlassian Document Format (ADF) JSON value to plain text.
pub fn render_adf(doc: &Value) -> String {
    let mut out = String::new();
    if let Some(content) = doc.get("content").and_then(|c| c.as_array()) {
        for node in content {
            render_node(node, &mut out, 0);
        }
    }
    out
}

fn render_node(node: &Value, out: &mut String, indent: usize) {
    let node_type = node.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match node_type {
        "text" => {
            let text = node.get("text").and_then(|t| t.as_str()).unwrap_or("");
            // Check for code mark
            let is_code = node
                .get("marks")
                .and_then(|m| m.as_array())
                .map(|marks| marks.iter().any(|m| m.get("type").and_then(|t| t.as_str()) == Some("code")))
                .unwrap_or(false);
            if is_code {
                out.push('`');
                out.push_str(text);
                out.push('`');
            } else {
                out.push_str(text);
            }
        }
        "heading" => {
            let level = node
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_u64())
                .unwrap_or(1);
            out.push('\n');
            for _ in 0..level {
                out.push('#');
            }
            out.push(' ');
            render_children(node, out, indent);
            out.push('\n');
        }
        "paragraph" => {
            render_indent(out, indent);
            render_children(node, out, indent);
            out.push('\n');
        }
        "bulletList" => {
            if let Some(items) = node.get("content").and_then(|c| c.as_array()) {
                for item in items {
                    render_indent(out, indent);
                    out.push_str("• ");
                    render_children(item, out, indent + 2);
                    out.push('\n');
                }
            }
        }
        "orderedList" => {
            if let Some(items) = node.get("content").and_then(|c| c.as_array()) {
                for (i, item) in items.iter().enumerate() {
                    render_indent(out, indent);
                    out.push_str(&format!("{}. ", i + 1));
                    render_children(item, out, indent + 3);
                    out.push('\n');
                }
            }
        }
        "taskList" => {
            if let Some(items) = node.get("content").and_then(|c| c.as_array()) {
                for item in items {
                    let state = item
                        .get("attrs")
                        .and_then(|a| a.get("state"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("TODO");
                    let checkbox = if state == "DONE" { "[x]" } else { "[ ]" };
                    render_indent(out, indent);
                    out.push_str(checkbox);
                    out.push(' ');
                    render_children(item, out, indent + 4);
                    out.push('\n');
                }
            }
        }
        "codeBlock" => {
            out.push_str("```\n");
            render_children(node, out, 0);
            out.push_str("\n```\n");
        }
        "blockquote" => {
            // Render children with > prefix
            let mut inner = String::new();
            render_children(node, &mut inner, 0);
            for line in inner.lines() {
                render_indent(out, indent);
                out.push_str("> ");
                out.push_str(line);
                out.push('\n');
            }
        }
        "hardBreak" => {
            out.push('\n');
        }
        "rule" => {
            out.push_str("---\n");
        }
        "mediaGroup" | "mediaSingle" | "media" => {
            out.push_str("[media]\n");
        }
        "inlineCard" | "blockCard" => {
            if let Some(url) = node.get("attrs").and_then(|a| a.get("url")).and_then(|u| u.as_str()) {
                out.push_str(url);
            }
        }
        _ => {
            // For unknown types, try to render children
            render_children(node, out, indent);
        }
    }
}

fn render_children(node: &Value, out: &mut String, indent: usize) {
    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
        for child in content {
            render_node(child, out, indent);
        }
    }
}

fn render_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push(' ');
    }
}

/// Render comments from the ADF comment field.
pub fn render_comments(comment_value: &Value) -> String {
    let mut out = String::new();
    let comments = comment_value
        .get("comments")
        .and_then(|c| c.as_array());

    if let Some(comments) = comments {
        for comment in comments {
            let author = comment
                .get("author")
                .and_then(|a| a.get("displayName"))
                .and_then(|n| n.as_str())
                .unwrap_or("Unknown");
            let created = comment
                .get("created")
                .and_then(|c| c.as_str())
                .unwrap_or("");
            // Take first 10 chars (date portion)
            let date = &created[..created.len().min(10)];
            out.push_str(&format!("--- {} ({}) ---\n", author, date));
            if let Some(body) = comment.get("body") {
                out.push_str(&render_adf(body));
            }
            out.push('\n');
        }
    }
    out
}
