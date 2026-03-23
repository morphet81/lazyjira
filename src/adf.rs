use serde_json::{json, Value};

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
            let marks = node.get("marks").and_then(|m| m.as_array());
            let has_mark = |name: &str| -> bool {
                marks
                    .map(|ms| ms.iter().any(|m| m.get("type").and_then(|t| t.as_str()) == Some(name)))
                    .unwrap_or(false)
            };
            let is_code = has_mark("code");
            let is_bold = has_mark("strong");
            let is_italic = has_mark("em");

            if is_code {
                out.push('`');
                out.push_str(text);
                out.push('`');
            } else {
                if is_bold { out.push_str("**"); }
                if is_italic { out.push('*'); }
                out.push_str(text);
                if is_italic { out.push('*'); }
                if is_bold { out.push_str("**"); }
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
                    out.push_str("- ");
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

// --- Markdown-like text → ADF conversion ---

/// Convert markdown-like text (as produced by render_adf) back to ADF JSON.
pub fn text_to_adf(text: &str) -> Value {
    let lines: Vec<&str> = text.lines().collect();
    let mut content: Vec<Value> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Empty line — skip
        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        // Code block: ```
        if line.trim_start().starts_with("```") {
            i += 1;
            let mut code_lines = Vec::new();
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1; // skip closing ```
            }
            content.push(json!({
                "type": "codeBlock",
                "content": [{ "type": "text", "text": code_lines.join("\n") }]
            }));
            continue;
        }

        // Heading: # , ## , ### , etc.
        if let Some(heading) = parse_heading(line) {
            content.push(heading);
            i += 1;
            continue;
        }

        // Rule: ---
        if line.trim() == "---" {
            content.push(json!({ "type": "rule" }));
            i += 1;
            continue;
        }

        // Bullet list: lines starting with -
        if line.trim_start().starts_with("- ") {
            let mut items = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with("- ") {
                let item_text = lines[i].trim_start().strip_prefix("- ").unwrap_or("");
                items.push(json!({
                    "type": "listItem",
                    "content": [{ "type": "paragraph", "content": parse_inline(item_text) }]
                }));
                i += 1;
            }
            content.push(json!({ "type": "bulletList", "content": items }));
            continue;
        }

        // Ordered list: lines starting with N.
        if is_ordered_list_item(line) {
            let mut items = Vec::new();
            while i < lines.len() && is_ordered_list_item(lines[i]) {
                let item_text = strip_ordered_prefix(lines[i]);
                items.push(json!({
                    "type": "listItem",
                    "content": [{ "type": "paragraph", "content": parse_inline(item_text) }]
                }));
                i += 1;
            }
            content.push(json!({ "type": "orderedList", "content": items }));
            continue;
        }

        // Task list: lines starting with [ ] or [x]
        if line.trim_start().starts_with("[ ] ") || line.trim_start().starts_with("[x] ") {
            let mut items = Vec::new();
            while i < lines.len()
                && (lines[i].trim_start().starts_with("[ ] ")
                    || lines[i].trim_start().starts_with("[x] "))
            {
                let trimmed = lines[i].trim_start();
                let (state, text) = if trimmed.starts_with("[x] ") {
                    ("DONE", &trimmed[4..])
                } else {
                    ("TODO", &trimmed[4..])
                };
                items.push(json!({
                    "type": "taskItem",
                    "attrs": { "state": state },
                    "content": parse_inline(text)
                }));
                i += 1;
            }
            content.push(json!({ "type": "taskList", "content": items }));
            continue;
        }

        // Blockquote: lines starting with >
        if line.trim_start().starts_with("> ") {
            let mut quote_lines = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with("> ") {
                let inner = lines[i].trim_start().strip_prefix("> ").unwrap_or("");
                quote_lines.push(inner);
                i += 1;
            }
            let inner_text = quote_lines.join("\n");
            let inner_adf = text_to_adf(&inner_text);
            let inner_content = inner_adf.get("content").cloned().unwrap_or(json!([]));
            content.push(json!({ "type": "blockquote", "content": inner_content }));
            continue;
        }

        // Default: paragraph (collect consecutive non-special lines)
        let mut para_lines = Vec::new();
        while i < lines.len() {
            let l = lines[i];
            if l.trim().is_empty()
                || l.trim_start().starts_with("```")
                || parse_heading(l).is_some()
                || l.trim() == "---"
                || l.trim_start().starts_with("- ")
                || is_ordered_list_item(l)
                || l.trim_start().starts_with("[ ] ")
                || l.trim_start().starts_with("[x] ")
                || l.trim_start().starts_with("> ")
            {
                break;
            }
            para_lines.push(l);
            i += 1;
        }
        let para_text = para_lines.join("\n");
        content.push(json!({
            "type": "paragraph",
            "content": parse_inline(&para_text)
        }));
    }

    json!({
        "version": 1,
        "type": "doc",
        "content": content
    })
}

fn parse_heading(line: &str) -> Option<Value> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let mut level = 0u64;
    for ch in trimmed.chars() {
        if ch == '#' {
            level += 1;
        } else {
            break;
        }
    }
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &trimmed[level as usize..];
    if !rest.starts_with(' ') {
        return None;
    }
    let text = rest.trim_start();
    Some(json!({
        "type": "heading",
        "attrs": { "level": level },
        "content": parse_inline(text)
    }))
}

fn is_ordered_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars();
    // Must start with digit(s), then ". "
    let first = chars.next();
    match first {
        Some(c) if c.is_ascii_digit() => {}
        _ => return false,
    }
    for ch in chars {
        if ch == '.' {
            // Next must be space
            return trimmed[trimmed.find('.').unwrap() + 1..].starts_with(' ');
        }
        if !ch.is_ascii_digit() {
            return false;
        }
    }
    false
}

fn strip_ordered_prefix(line: &str) -> &str {
    let trimmed = line.trim_start();
    if let Some(dot_pos) = trimmed.find(". ") {
        &trimmed[dot_pos + 2..]
    } else {
        trimmed
    }
}

/// Parse inline markdown text, handling `code`, **bold**, and *italic* spans.
fn parse_inline(text: &str) -> Vec<Value> {
    let mut nodes = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut plain = String::new();

    let flush_plain = |plain: &mut String, nodes: &mut Vec<Value>| {
        if !plain.is_empty() {
            nodes.push(json!({ "type": "text", "text": plain.as_str() }));
            plain.clear();
        }
    };

    while i < len {
        // Code span: `...`
        if chars[i] == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                flush_plain(&mut plain, &mut nodes);
                let code: String = chars[i + 1..i + 1 + end].iter().collect();
                nodes.push(json!({
                    "type": "text",
                    "text": code,
                    "marks": [{ "type": "code" }]
                }));
                i += end + 2;
                continue;
            }
        }

        // Bold: **...**
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(end) = find_closing(&chars, i + 2, "**") {
                flush_plain(&mut plain, &mut nodes);
                let inner: String = chars[i + 2..end].iter().collect();
                // Check for nested italic inside bold: ***text*** → bold+italic
                let inner_nodes = parse_inline(&inner);
                for mut node in inner_nodes {
                    // Add "strong" mark to each node
                    add_mark(&mut node, "strong");
                    nodes.push(node);
                }
                i = end + 2;
                continue;
            }
        }

        // Italic: *...*  (but not **)
        if chars[i] == '*' && !(i + 1 < len && chars[i + 1] == '*') {
            if let Some(end) = find_closing_single_star(&chars, i + 1) {
                flush_plain(&mut plain, &mut nodes);
                let inner: String = chars[i + 1..end].iter().collect();
                let inner_nodes = parse_inline(&inner);
                for mut node in inner_nodes {
                    add_mark(&mut node, "em");
                    nodes.push(node);
                }
                i = end + 1;
                continue;
            }
        }

        plain.push(chars[i]);
        i += 1;
    }

    flush_plain(&mut plain, &mut nodes);

    if nodes.is_empty() {
        nodes.push(json!({ "type": "text", "text": "" }));
    }

    nodes
}

fn find_closing(chars: &[char], start: usize, marker: &str) -> Option<usize> {
    let marker_chars: Vec<char> = marker.chars().collect();
    let mlen = marker_chars.len();
    let mut i = start;
    while i + mlen <= chars.len() {
        if chars[i..i + mlen] == marker_chars[..] {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_closing_single_star(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i < chars.len() {
        if chars[i] == '*' && !(i + 1 < chars.len() && chars[i + 1] == '*') {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn add_mark(node: &mut Value, mark_type: &str) {
    if let Some(obj) = node.as_object_mut() {
        let marks = obj
            .entry("marks")
            .or_insert_with(|| json!([]))
            .as_array_mut();
        if let Some(marks) = marks {
            marks.push(json!({ "type": mark_type }));
        }
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
