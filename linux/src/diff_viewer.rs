use serde_json::Value;
use std::fs;
use std::path::Path;

const MAX_REVIEW_COMMENTS: usize = 512;
const MAX_COMMENT_FIELD_CHARS: usize = 16_384;
const MAX_COMMENT_META_CHARS: usize = 1_024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffReviewComment {
    pub id: Option<String>,
    pub file_path: String,
    pub side: String,
    pub start_line: Option<u64>,
    pub end_line: Option<u64>,
    pub end_side: Option<String>,
    pub line_text: Option<String>,
    pub message: String,
    pub submission_text: Option<String>,
    pub author: Option<String>,
    pub created_at: Option<String>,
    pub outdated: bool,
    pub resolved: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDiffRowKind {
    Header,
    Hunk,
    Context,
    Change,
    Meta,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitDiffRow {
    pub old_line: Option<u64>,
    pub new_line: Option<u64>,
    pub old_text: String,
    pub new_text: String,
    pub kind: SplitDiffRowKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitDiffSection {
    pub path: String,
    pub rows: Vec<SplitDiffRow>,
}

pub fn review_comments_from_params(params: &Value) -> Result<Vec<DiffReviewComment>, String> {
    let mut comments = Vec::new();
    if let Some(value) = params
        .get("comments")
        .or_else(|| params.get("review_comments"))
    {
        append_comments_from_value(value, "comments", &mut comments)?;
    }
    if let Some(path) = string_param(params, "comments_file")
        .or_else(|| string_param(params, "review_comments_file"))
        .or_else(|| string_param(params, "review_comments_path"))
    {
        append_comments_from_file(Path::new(&path), &mut comments)?;
    }
    ensure_comment_limit(comments.len())?;
    Ok(comments)
}

pub fn review_comments_from_cli(
    comments_file: Option<&str>,
    comments_json: Option<&str>,
) -> Result<Vec<DiffReviewComment>, String> {
    let mut comments = Vec::new();
    if let Some(raw_json) = comments_json {
        let value = parse_comment_json(raw_json, "--comments-json")?;
        append_comments_from_value(&value, "--comments-json", &mut comments)?;
    }
    if let Some(raw_path) = comments_file {
        append_comments_from_file(Path::new(raw_path), &mut comments)?;
    }
    ensure_comment_limit(comments.len())?;
    Ok(comments)
}

pub fn render_diff_document_with_layout(
    diff: &str,
    title: &str,
    source_label: &str,
    font_size: f64,
    comments: &[DiffReviewComment],
    layout: &str,
) -> String {
    let diff_body = if layout == "split" {
        render_split_diff_html(diff)
    } else {
        let rows = diff
            .lines()
            .map(render_diff_line)
            .collect::<Vec<_>>()
            .join("");
        format!("<pre class=\"unified-diff\">{rows}</pre>")
    };
    let comments_html = render_review_comments(comments);
    let escaped_title = html_escape(title);
    let escaped_source = html_escape(source_label);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{escaped_title}</title><style>\
         :root {{ color-scheme: light dark; }}\
         body {{ margin: 0; font: 14px/1.4 system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif; background: #f6f8fa; color: #1f2328; }}\
         header {{ position: sticky; top: 0; z-index: 1; background: #ffffff; border-bottom: 1px solid #d0d7de; padding: 14px 18px; }}\
         h1 {{ margin: 0; font-size: 18px; }}\
         .source {{ margin-top: 4px; color: #57606a; font-size: 12px; }}\
         .review-comments {{ padding: 14px 18px 16px; background: #f6f8fa; border-bottom: 1px solid #d0d7de; }}\
         .review-comments h2 {{ margin: 0 0 10px; font-size: 14px; font-weight: 700; }}\
         .review-comments ol {{ display: grid; gap: 8px; list-style: none; margin: 0; max-width: 1040px; padding: 0; }}\
         .review-comment {{ background: #ffffff; border: 1px solid #d0d7de; border-left: 4px solid #0969da; border-radius: 6px; padding: 10px 12px; }}\
         .review-comment.deletions {{ border-left-color: #cf222e; }}\
         .comment-meta {{ align-items: center; color: #57606a; display: flex; flex-wrap: wrap; gap: 6px; font-size: 12px; }}\
         .comment-file {{ color: #24292f; font-weight: 700; }}\
         .comment-badge {{ border: 1px solid #d0d7de; border-radius: 999px; color: #57606a; padding: 1px 6px; }}\
         .comment-message {{ margin: 8px 0 0; white-space: pre-wrap; font: 13px/1.45 system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif; }}\
         .comment-line-text {{ margin-top: 8px; overflow: auto; padding: 6px 8px; white-space: pre; background: #f6f8fa; border-radius: 4px; color: #57606a; font: 12px/1.45 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }}\
         pre {{ margin: 0; font: {font_size}px/1.45 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }}\
         .unified-diff {{ padding: 16px 0 32px; overflow: auto; background: #ffffff; }}\
         .line {{ display: block; white-space: pre; padding: 0 18px; }}\
         .file {{ background: #eef4ff; color: #0969da; font-weight: 600; }}\
         .hunk {{ background: #ddf4ff; color: #0550ae; }}\
         .add {{ background: #dafbe1; color: #116329; }}\
         .del {{ background: #ffebe9; color: #82071e; }}\
         .split-diff {{ background: #ffffff; overflow: auto; padding: 12px 0 32px; }}\
         .split-file {{ border-bottom: 1px solid #d0d7de; margin-bottom: 14px; }}\
         .split-file h2 {{ background: #eef4ff; color: #0969da; font: 600 13px/1.4 system-ui, sans-serif; margin: 0; padding: 8px 14px; }}\
         .split-table {{ border-collapse: collapse; table-layout: fixed; width: 100%; min-width: 900px; }}\
         .split-table td {{ font: {font_size}px/1.45 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; padding: 0 8px; vertical-align: top; white-space: pre; }}\
         .split-table .line-number {{ color: #6e7781; padding-left: 6px; padding-right: 6px; text-align: right; user-select: none; width: 4em; }}\
         .split-table .old-code {{ border-right: 1px solid #d0d7de; width: calc(50% - 4em); }}\
         .split-table .new-code {{ width: calc(50% - 4em); }}\
         .split-row.hunk td {{ background: #ddf4ff; color: #0550ae; }}\
         .split-row.header td {{ background: #eef4ff; color: #0969da; font-weight: 600; }}\
         .split-row.meta td {{ color: #6e7781; }}\
         .split-row.change .old-number, .split-row.change .old-code {{ background: #ffebe9; color: #82071e; }}\
         .split-row.change .new-number, .split-row.change .new-code {{ background: #dafbe1; color: #116329; }}\
         @media (prefers-color-scheme: dark) {{\
           body, .review-comments {{ background: #0d1117; color: #e6edf3; }}\
           header, .unified-diff, .split-diff, .review-comment {{ background: #010409; border-color: #30363d; }}\
           .source, .comment-meta, .comment-badge, .comment-line-text {{ color: #8b949e; }}\
           .comment-file {{ color: #e6edf3; }}\
           .comment-badge {{ border-color: #30363d; }}\
           .comment-line-text {{ background: #0d1117; }}\
           .file {{ background: #111d2f; color: #79c0ff; }}\
           .hunk {{ background: #0f2a3d; color: #a5d6ff; }}\
           .add {{ background: #12261d; color: #7ee787; }}\
           .del {{ background: #2d1518; color: #ffa198; }}\
           .split-file {{ border-color: #30363d; }}\
           .split-file h2, .split-row.header td {{ background: #111d2f; color: #79c0ff; }}\
           .split-table .line-number, .split-row.meta td {{ color: #8b949e; }}\
           .split-table .old-code {{ border-color: #30363d; }}\
           .split-row.hunk td {{ background: #0f2a3d; color: #a5d6ff; }}\
           .split-row.change .old-number, .split-row.change .old-code {{ background: #2d1518; color: #ffa198; }}\
           .split-row.change .new-number, .split-row.change .new-code {{ background: #12261d; color: #7ee787; }}\
         }}\
         </style></head><body><header><h1>{escaped_title}</h1><div class=\"source\">{escaped_source}</div></header>{comments_html}<main id=\"diff-content\">{diff_body}</main></body></html>"
    )
}

pub fn replace_diff_document_layout(html: &str, diff: &str, layout: &str) -> Option<String> {
    const START: &str = "<main id=\"diff-content\">";
    const END: &str = "</main>";
    let start = html.find(START)? + START.len();
    let end = html[start..].find(END)? + start;
    let mut updated = String::with_capacity(html.len() + diff.len());
    updated.push_str(&html[..start]);
    let body = if layout == "split" {
        render_split_diff_html(diff)
    } else {
        format!(
            "<pre class=\"unified-diff\">{}</pre>",
            diff.lines()
                .map(render_diff_line)
                .collect::<Vec<_>>()
                .join("")
        )
    };
    updated.push_str(&body);
    updated.push_str(&html[end..]);
    Some(updated)
}

fn append_comments_from_file(
    path: &Path,
    comments: &mut Vec<DiffReviewComment>,
) -> Result<(), String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read review comments {}: {err}", path.display()))?;
    let value = parse_comment_json(&text, &path.display().to_string())?;
    append_comments_from_value(&value, &path.display().to_string(), comments)
}

fn parse_comment_json(raw_json: &str, source: &str) -> Result<Value, String> {
    if raw_json.trim().is_empty() {
        return Err(format!("{source} was empty"));
    }
    serde_json::from_str(raw_json).map_err(|err| format!("failed to parse {source}: {err}"))
}

fn append_comments_from_value(
    value: &Value,
    source: &str,
    comments: &mut Vec<DiffReviewComment>,
) -> Result<(), String> {
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                comments.push(parse_comment(item, &format!("{source}[{index}]"))?);
                ensure_comment_limit(comments.len())?;
            }
            Ok(())
        }
        Value::Object(object) => {
            if let Some(comments_value) = object
                .get("comments")
                .or_else(|| object.get("review_comments"))
            {
                append_comments_from_value(comments_value, source, comments)
            } else {
                comments.push(parse_comment(value, source)?);
                ensure_comment_limit(comments.len())
            }
        }
        _ => Err(format!(
            "{source} must be a comment object, an array, or an object with a comments array"
        )),
    }
}

fn ensure_comment_limit(count: usize) -> Result<(), String> {
    if count > MAX_REVIEW_COMMENTS {
        Err(format!(
            "review comments exceed limit of {MAX_REVIEW_COMMENTS}"
        ))
    } else {
        Ok(())
    }
}

fn parse_comment(value: &Value, source: &str) -> Result<DiffReviewComment, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{source} must be an object"))?;
    let file_path = required_string(
        object,
        &["filePath", "file_path", "path"],
        "filePath",
        MAX_COMMENT_META_CHARS,
        source,
    )?;
    let message = required_string(
        object,
        &["message", "body", "text"],
        "message",
        MAX_COMMENT_FIELD_CHARS,
        source,
    )?;
    let start_line = optional_positive_int(
        object,
        &[
            "startLine",
            "start_line",
            "line",
            "lineNumber",
            "line_number",
            "originalLine",
            "original_line",
            "position",
        ],
        source,
    )?;
    let end_line = optional_positive_int(
        object,
        &[
            "endLine",
            "end_line",
            "line",
            "lineNumber",
            "line_number",
            "originalLine",
            "original_line",
            "position",
        ],
        source,
    )?
    .or(start_line);

    Ok(DiffReviewComment {
        id: optional_string(object, &["id"], MAX_COMMENT_META_CHARS, source)?,
        file_path,
        side: normalize_side(
            optional_string(object, &["side"], MAX_COMMENT_META_CHARS, source)?
                .as_deref()
                .unwrap_or("additions"),
        ),
        start_line,
        end_line,
        end_side: optional_string(
            object,
            &["endSide", "end_side"],
            MAX_COMMENT_META_CHARS,
            source,
        )?,
        line_text: optional_string(
            object,
            &["lineText", "line_text"],
            MAX_COMMENT_FIELD_CHARS,
            source,
        )?,
        message,
        submission_text: optional_string(
            object,
            &["submissionText", "submission_text"],
            MAX_COMMENT_FIELD_CHARS,
            source,
        )?,
        author: optional_author(object, source)?,
        created_at: optional_string(
            object,
            &["createdAt", "created_at"],
            MAX_COMMENT_META_CHARS,
            source,
        )?,
        outdated: optional_bool(object, &["outdated", "isOutdated", "is_outdated"]),
        resolved: optional_bool(object, &["resolved", "isResolved", "is_resolved"]),
    })
}

fn render_review_comments(comments: &[DiffReviewComment]) -> String {
    if comments.is_empty() {
        return String::new();
    }
    let mut html = format!(
        "<section class=\"review-comments\" aria-label=\"Review comments\"><h2>Review comments ({})</h2><ol>",
        comments.len()
    );
    for comment in comments {
        let side_class = if comment.side == "deletions" {
            "deletions"
        } else {
            "additions"
        };
        html.push_str(&format!(
            "<li class=\"review-comment {side_class}\" data-comment-id=\"{}\"><div class=\"comment-meta\"><span class=\"comment-file\">{}</span><span>{}</span>",
            html_escape_attr(comment.id.as_deref().unwrap_or_default()),
            html_escape(&comment.file_path),
            html_escape(&comment_location(comment))
        ));
        if let Some(author) = comment.author.as_deref() {
            html.push_str(&format!("<span>by {}</span>", html_escape(author)));
        }
        if let Some(created_at) = comment.created_at.as_deref() {
            html.push_str(&format!("<span>{}</span>", html_escape(created_at)));
        }
        if comment.outdated {
            html.push_str("<span class=\"comment-badge\">Outdated</span>");
        }
        if comment.resolved {
            html.push_str("<span class=\"comment-badge\">Resolved</span>");
        }
        html.push_str("</div>");
        html.push_str(&format!(
            "<pre class=\"comment-message\">{}</pre>",
            html_escape(&comment.message)
        ));
        if let Some(line_text) = comment.line_text.as_deref() {
            html.push_str(&format!(
                "<div class=\"comment-line-text\">{}</div>",
                html_escape(line_text)
            ));
        }
        if let Some(submission_text) = comment.submission_text.as_deref() {
            if submission_text.trim() != comment.message.trim() {
                html.push_str(&format!(
                    "<div class=\"comment-line-text\">{}</div>",
                    html_escape(submission_text)
                ));
            }
        }
        html.push_str("</li>");
    }
    html.push_str("</ol></section>");
    html
}

fn comment_location(comment: &DiffReviewComment) -> String {
    let side = if comment.side == "deletions" {
        "deletions"
    } else {
        "additions"
    };
    match (comment.start_line, comment.end_line) {
        (Some(start), Some(end)) if start != end => {
            format!("{side} lines {start}-{end}")
        }
        (Some(line), _) | (_, Some(line)) => format!("{side} line {line}"),
        _ => side.to_string(),
    }
}

fn render_diff_line(line: &str) -> String {
    let class = if line.starts_with("diff --git") || line.starts_with("Index: ") {
        "file"
    } else if line.starts_with("@@") {
        "hunk"
    } else if line.starts_with('+') && !line.starts_with("+++") {
        "add"
    } else if line.starts_with('-') && !line.starts_with("---") {
        "del"
    } else {
        "ctx"
    };
    format!(
        "<span class=\"line {class}\">{}</span>\n",
        html_escape(line)
    )
}

pub fn split_diff_sections(source: &str) -> Vec<SplitDiffSection> {
    diff_sections(source)
        .into_iter()
        .map(|(path, content)| SplitDiffSection {
            path,
            rows: split_diff_rows(&content),
        })
        .collect()
}

fn diff_sections(source: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let mut path = "Overview".to_string();
    let mut lines = Vec::new();
    for line in source.lines() {
        if let Some(next_path) = diff_header_path(line) {
            if !lines.is_empty() {
                sections.push((path, format!("{}\n", lines.join("\n"))));
            }
            path = next_path;
            lines.clear();
        }
        lines.push(line);
    }
    if !lines.is_empty() {
        sections.push((path, format!("{}\n", lines.join("\n"))));
    }
    if sections.is_empty() {
        sections.push(("Diff".to_string(), source.to_string()));
    }
    sections
}

pub fn diff_header_path(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix("diff --git ") {
        let paths = shell_words::split(rest)
            .unwrap_or_else(|_| rest.split_whitespace().map(ToString::to_string).collect());
        return paths.get(1).or_else(|| paths.first()).map(|path| {
            path.trim_matches('"')
                .strip_prefix("b/")
                .unwrap_or(path)
                .to_string()
        });
    }
    line.strip_prefix("Index: ")
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
}

fn split_diff_rows(content: &str) -> Vec<SplitDiffRow> {
    let lines = content.lines().collect::<Vec<_>>();
    let mut rows = Vec::new();
    let mut old_line = None;
    let mut new_line = None;
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if let Some((old_start, new_start)) = parse_hunk_starts(line) {
            old_line = Some(old_start);
            new_line = Some(new_start);
            rows.push(shared_split_row(line, SplitDiffRowKind::Hunk));
            index += 1;
            continue;
        }
        if old_line.is_some() && is_deletion_line(line) {
            let deletion_start = index;
            while index < lines.len() && is_deletion_line(lines[index]) {
                index += 1;
            }
            let addition_start = index;
            while index < lines.len() && is_addition_line(lines[index]) {
                index += 1;
            }
            append_change_rows(
                &mut rows,
                &lines[deletion_start..addition_start],
                &lines[addition_start..index],
                &mut old_line,
                &mut new_line,
            );
            continue;
        }
        if old_line.is_some() && is_addition_line(line) {
            let addition_start = index;
            while index < lines.len() && is_addition_line(lines[index]) {
                index += 1;
            }
            append_change_rows(
                &mut rows,
                &[],
                &lines[addition_start..index],
                &mut old_line,
                &mut new_line,
            );
            continue;
        }
        if old_line.is_some() && line.starts_with(' ') {
            let text = line.strip_prefix(' ').unwrap_or(line).to_string();
            rows.push(SplitDiffRow {
                old_line,
                new_line,
                old_text: text.clone(),
                new_text: text,
                kind: SplitDiffRowKind::Context,
            });
            increment_line(&mut old_line);
            increment_line(&mut new_line);
            index += 1;
            continue;
        }
        if line.starts_with('\\') {
            rows.push(shared_split_row(line, SplitDiffRowKind::Meta));
        } else {
            rows.push(shared_split_row(line, SplitDiffRowKind::Header));
        }
        index += 1;
    }
    rows
}

fn append_change_rows(
    rows: &mut Vec<SplitDiffRow>,
    deletions: &[&str],
    additions: &[&str],
    old_line: &mut Option<u64>,
    new_line: &mut Option<u64>,
) {
    for index in 0..deletions.len().max(additions.len()) {
        let deletion = deletions.get(index);
        let addition = additions.get(index);
        let row_old_line = deletion.map(|_| *old_line).flatten();
        let row_new_line = addition.map(|_| *new_line).flatten();
        rows.push(SplitDiffRow {
            old_line: row_old_line,
            new_line: row_new_line,
            old_text: deletion
                .map(|line| line.strip_prefix('-').unwrap_or(line).to_string())
                .unwrap_or_default(),
            new_text: addition
                .map(|line| line.strip_prefix('+').unwrap_or(line).to_string())
                .unwrap_or_default(),
            kind: SplitDiffRowKind::Change,
        });
        if deletion.is_some() {
            increment_line(old_line);
        }
        if addition.is_some() {
            increment_line(new_line);
        }
    }
}

fn shared_split_row(text: &str, kind: SplitDiffRowKind) -> SplitDiffRow {
    SplitDiffRow {
        old_line: None,
        new_line: None,
        old_text: text.to_string(),
        new_text: text.to_string(),
        kind,
    }
}

fn increment_line(line: &mut Option<u64>) {
    if let Some(value) = line.as_mut() {
        *value = value.saturating_add(1);
    }
}

fn is_deletion_line(line: &str) -> bool {
    line.starts_with('-') && !line.starts_with("---")
}

fn is_addition_line(line: &str) -> bool {
    line.starts_with('+') && !line.starts_with("+++")
}

fn parse_hunk_starts(line: &str) -> Option<(u64, u64)> {
    if !line.starts_with("@@ ") {
        return None;
    }
    let mut parts = line.split_whitespace();
    parts.next()?;
    let old = parse_hunk_range(parts.next()?, '-')?;
    let new = parse_hunk_range(parts.next()?, '+')?;
    Some((old, new))
}

fn parse_hunk_range(value: &str, prefix: char) -> Option<u64> {
    value
        .strip_prefix(prefix)?
        .split(',')
        .next()?
        .parse::<u64>()
        .ok()
}

fn render_split_diff_html(source: &str) -> String {
    let mut html = String::from("<div class=\"split-diff\">");
    for section in split_diff_sections(source) {
        html.push_str(&format!(
            "<section class=\"split-file\"><h2>{}</h2><table class=\"split-table\"><tbody>",
            html_escape(&section.path)
        ));
        for row in section.rows {
            let class = match row.kind {
                SplitDiffRowKind::Header => "header",
                SplitDiffRowKind::Hunk => "hunk",
                SplitDiffRowKind::Context => "context",
                SplitDiffRowKind::Change => "change",
                SplitDiffRowKind::Meta => "meta",
            };
            html.push_str(&format!(
                "<tr class=\"split-row {class}\"><td class=\"line-number old-number\">{}</td><td class=\"old-code\">{}</td><td class=\"line-number new-number\">{}</td><td class=\"new-code\">{}</td></tr>",
                row.old_line.map(|line| line.to_string()).unwrap_or_default(),
                html_escape(&row.old_text),
                row.new_line.map(|line| line.to_string()).unwrap_or_default(),
                html_escape(&row.new_text)
            ));
        }
        html.push_str("</tbody></table></section>");
    }
    html.push_str("</div>");
    html
}

fn required_string(
    object: &serde_json::Map<String, Value>,
    keys: &[&str],
    field: &str,
    max_chars: usize,
    source: &str,
) -> Result<String, String> {
    optional_string(object, keys, max_chars, source)?.ok_or_else(|| {
        format!(
            "{source} is missing required {field}; accepted keys: {}",
            keys.join(", ")
        )
    })
}

fn optional_string(
    object: &serde_json::Map<String, Value>,
    keys: &[&str],
    max_chars: usize,
    source: &str,
) -> Result<Option<String>, String> {
    for key in keys {
        if let Some(value) = object.get(*key) {
            if value.is_null() {
                continue;
            }
            let Some(raw) = value.as_str() else {
                return Err(format!("{source}.{key} must be a string"));
            };
            return bounded_optional_string(raw, max_chars, source, key);
        }
    }
    Ok(None)
}

fn optional_author(
    object: &serde_json::Map<String, Value>,
    source: &str,
) -> Result<Option<String>, String> {
    for key in ["author", "user"] {
        let Some(value) = object.get(key) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        if let Some(raw) = value.as_str() {
            return bounded_optional_string(raw, MAX_COMMENT_META_CHARS, source, key);
        }
        if let Some(author) = value.as_object() {
            for nested_key in ["login", "name", "username"] {
                if let Some(raw) = author.get(nested_key).and_then(Value::as_str) {
                    return bounded_optional_string(
                        raw,
                        MAX_COMMENT_META_CHARS,
                        source,
                        nested_key,
                    );
                }
            }
        }
        return Err(format!("{source}.{key} must be a string or user object"));
    }
    Ok(None)
}

fn bounded_optional_string(
    raw: &str,
    max_chars: usize,
    source: &str,
    key: &str,
) -> Result<Option<String>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > max_chars {
        return Err(format!("{source}.{key} exceeds {max_chars} characters"));
    }
    Ok(Some(trimmed.to_string()))
}

fn optional_positive_int(
    object: &serde_json::Map<String, Value>,
    keys: &[&str],
    source: &str,
) -> Result<Option<u64>, String> {
    for key in keys {
        if let Some(value) = object.get(*key) {
            if value.is_null() {
                continue;
            }
            let parsed = if let Some(raw) = value.as_u64() {
                raw
            } else if let Some(raw) = value.as_i64() {
                u64::try_from(raw).unwrap_or(0)
            } else if let Some(raw) = value.as_str() {
                raw.trim()
                    .parse::<u64>()
                    .map_err(|_| format!("{source}.{key} must be a positive integer"))?
            } else {
                return Err(format!("{source}.{key} must be a positive integer"));
            };
            if parsed == 0 {
                return Err(format!("{source}.{key} must be a positive integer"));
            }
            return Ok(Some(parsed));
        }
    }
    Ok(None)
}

fn optional_bool(object: &serde_json::Map<String, Value>, keys: &[&str]) -> bool {
    for key in keys {
        if let Some(value) = object.get(*key) {
            if let Some(raw) = value.as_bool() {
                return raw;
            }
            if let Some(raw) = value.as_str() {
                return matches!(
                    raw.trim().to_ascii_lowercase().as_str(),
                    "true" | "1" | "yes" | "on"
                );
            }
        }
    }
    false
}

fn normalize_side(side: &str) -> String {
    match side.trim().to_ascii_lowercase().as_str() {
        "deletions" | "deletion" | "deleted" | "left" | "old" | "base" => "deletions".to_string(),
        _ => "additions".to_string(),
    }
}

fn string_param(params: &Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn html_escape_attr(text: &str) -> String {
    html_escape(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "\
diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,4 +1,5 @@
 fn main() {
-    old();
-    removed();
+    new();
+    inserted();
+    extra();
 }
";

    #[test]
    fn split_diff_rows_align_replacements_and_line_numbers() {
        let sections = split_diff_sections(PATCH);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].path, "src/main.rs");
        let changes = sections[0]
            .rows
            .iter()
            .filter(|row| row.kind == SplitDiffRowKind::Change)
            .collect::<Vec<_>>();
        assert_eq!(changes.len(), 3);
        assert_eq!(
            (
                changes[0].old_line,
                changes[0].new_line,
                changes[0].old_text.as_str(),
                changes[0].new_text.as_str()
            ),
            (Some(2), Some(2), "    old();", "    new();")
        );
        assert_eq!(
            (
                changes[1].old_line,
                changes[1].new_line,
                changes[1].old_text.as_str(),
                changes[1].new_text.as_str()
            ),
            (Some(3), Some(3), "    removed();", "    inserted();")
        );
        assert_eq!(
            (
                changes[2].old_line,
                changes[2].new_line,
                changes[2].old_text.as_str(),
                changes[2].new_text.as_str()
            ),
            (None, Some(4), "", "    extra();")
        );
        let trailing_context = sections[0]
            .rows
            .iter()
            .rev()
            .find(|row| row.kind == SplitDiffRowKind::Context)
            .expect("trailing context");
        assert_eq!(
            (trailing_context.old_line, trailing_context.new_line),
            (Some(4), Some(5))
        );
    }

    #[test]
    fn split_diff_html_exposes_side_by_side_cells() {
        let html = render_diff_document_with_layout(PATCH, "Review", "fixture", 12.0, &[], "split");
        assert!(html.contains("class=\"split-diff\""));
        assert!(html.contains("class=\"line-number old-number\">2</td>"));
        assert!(html.contains("class=\"line-number new-number\">4</td>"));
        assert!(html.contains("class=\"old-code\">    old();</td>"));
        assert!(html.contains("class=\"new-code\">    extra();</td>"));
    }

    #[test]
    fn diff_header_paths_support_git_quoted_filenames() {
        assert_eq!(
            diff_header_path(r#"diff --git "a/docs/old name.md" "b/docs/new name.md""#),
            Some("docs/new name.md".to_string())
        );
    }

    #[test]
    fn replacing_layout_preserves_surrounding_diff_document_metadata() {
        let original = render_diff_document_with_layout(
            PATCH,
            "Review",
            "fixture",
            12.0,
            &[DiffReviewComment {
                id: Some("comment-1".to_string()),
                file_path: "src/main.rs".to_string(),
                side: "additions".to_string(),
                start_line: Some(2),
                end_line: Some(2),
                end_side: None,
                line_text: None,
                message: "Keep this comment".to_string(),
                submission_text: None,
                author: Some("reviewer".to_string()),
                created_at: None,
                outdated: false,
                resolved: false,
            }],
            "unified",
        );
        let replaced =
            replace_diff_document_layout(&original, PATCH, "split").expect("replace layout");
        assert!(replaced.contains("Keep this comment"));
        assert!(replaced.contains("by reviewer"));
        assert!(replaced.contains("class=\"split-diff\""));
        assert!(!replaced.contains("class=\"unified-diff\""));
    }
}
