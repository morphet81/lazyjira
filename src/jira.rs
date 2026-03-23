use anyhow::{Context, Result};
use serde::Deserialize;
use std::io::Write;
use std::process::Command;

use crate::adf;

#[derive(Debug, Clone, Deserialize)]
pub struct JiraProject {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatusCategory {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub name: String,
    pub status_category: StatusCategory,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignee {
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueType {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Priority {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct WorkItemFields {
    pub summary: String,
    pub status: Status,
    pub assignee: Option<Assignee>,
    pub issuetype: Option<IssueType>,
    pub priority: Option<Priority>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkItem {
    pub key: String,
    pub fields: WorkItemFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DetailFields {
    pub summary: String,
    pub status: Status,
    pub assignee: Option<Assignee>,
    pub issuetype: Option<IssueType>,
    pub priority: Option<Priority>,
    pub description: Option<serde_json::Value>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub comment: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkItemDetail {
    pub key: String,
    pub fields: DetailFields,
}

fn run_acli(args: &[&str]) -> Result<String> {
    let output = Command::new("acli")
        .args(args)
        .output()
        .context("Failed to execute acli")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("acli failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn fetch_projects() -> Result<Vec<JiraProject>> {
    let json = run_acli(&["jira", "project", "list", "--json", "--recent"])?;
    let projects: Vec<JiraProject> = serde_json::from_str(&json)?;
    Ok(projects)
}

pub fn fetch_workitems(project_key: &str, epic_key: Option<&str>) -> Result<Vec<WorkItem>> {
    let jql = match epic_key {
        Some(key) => format!("project = {} AND parent = {} ORDER BY rank", project_key, key),
        None => format!("project = {} ORDER BY rank", project_key),
    };
    let json = run_acli(&[
        "jira",
        "workitem",
        "search",
        "--jql",
        &jql,
        "--fields",
        "key,status,summary,assignee,issuetype,priority",
        "--limit",
        "200",
        "--json",
    ])?;
    let items: Vec<WorkItem> = serde_json::from_str(&json)?;
    Ok(items)
}

pub fn fetch_epics(project_key: &str) -> Result<Vec<WorkItem>> {
    let jql = format!(
        "project = {} AND issuetype = Epic AND statusCategory != Done ORDER BY rank",
        project_key
    );
    let json = run_acli(&[
        "jira",
        "workitem",
        "search",
        "--jql",
        &jql,
        "--fields",
        "key,status,summary,assignee,issuetype,priority",
        "--limit",
        "200",
        "--json",
    ])?;
    let items: Vec<WorkItem> = serde_json::from_str(&json)?;
    Ok(items)
}

pub fn update_workitem(key: &str, field: &str, value: &str) -> Result<()> {
    match field {
        "summary" => {
            run_acli(&[
                "jira", "workitem", "edit", "--key", key, "--summary", value, "--yes",
            ])?;
        }
        "description" => {
            // Convert markdown-like text to ADF and save via --from-json
            let adf_doc = adf::text_to_adf(value);
            let payload = serde_json::json!({
                "issues": [key],
                "description": adf_doc
            });
            let mut tmp = tempfile::NamedTempFile::new()
                .context("Failed to create temp file")?;
            write!(tmp, "{}", serde_json::to_string(&payload)?)
                .context("Failed to write temp file")?;
            let tmp_path = tmp.path().to_string_lossy().to_string();
            run_acli(&[
                "jira", "workitem", "edit", "--from-json", &tmp_path, "--yes",
            ])?;
        }
        _ => anyhow::bail!("Unknown editable field: {}", field),
    }
    Ok(())
}

pub fn fetch_workitem_detail(key: &str) -> Result<WorkItemDetail> {
    let json = run_acli(&[
        "jira",
        "workitem",
        "view",
        key,
        "--json",
        "--fields",
        "key,issuetype,summary,status,assignee,description,priority,created,updated,comment",
    ])?;
    // acli returns a single object (not array) for view
    let detail: WorkItemDetail = serde_json::from_str(&json)?;
    Ok(detail)
}
