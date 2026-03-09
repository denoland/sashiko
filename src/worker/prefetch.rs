use anyhow::Result;
use regex::Regex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tokio::fs;
use tracing::info;
use tree_sitter::{Parser, Point};

#[derive(Serialize)]
struct PrefetchRangeLog {
    start_line: usize,
    end_line: usize,
}

#[derive(Serialize)]
struct PrefetchFileLog {
    filename: String,
    ranges: Vec<PrefetchRangeLog>,
}

/// Parses a unified diff and returns a map of filename -> list of modified line ranges.
/// Line numbers are 0-based to align with Tree-sitter's Point API.
pub fn parse_diff_ranges(diff: &str) -> HashMap<String, Vec<(usize, usize)>> {
    let mut files = HashMap::new();
    let mut current_file = None;

    let chunk_header_re = Regex::new(r"@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@").unwrap();
    for line in diff.lines() {
        if let Some(fname) = line.strip_prefix("+++ b/") {
            let fname = fname.to_string();
            current_file = Some(fname.clone());
            files.entry(fname).or_insert_with(Vec::new);
        } else if line.starts_with("@@")
            && let Some(fname) = &current_file
            && let Some(caps) = chunk_header_re.captures(line)
        {
            let start: usize = caps
                .get(1)
                .map(|m| m.as_str().parse().unwrap_or(1))
                .unwrap_or(1);
            let count: usize = caps
                .get(2)
                .map(|m| m.as_str().parse().unwrap_or(1))
                .unwrap_or(1);
            if count > 0 {
                // Convert to 0-based indices for tree-sitter
                let start_0 = start.saturating_sub(1);
                let end_0 = start_0 + count.saturating_sub(1);
                files.get_mut(fname).unwrap().push((start_0, end_0));
            }
        }
    }

    // Merge overlapping/adjacent ranges (within 10 lines)
    for ranges in files.values_mut() {
        ranges.sort_by_key(|r| r.0);
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for r in ranges.iter() {
            if let Some(last) = merged.last_mut() {
                if r.0 <= last.1 + 10 {
                    last.1 = std::cmp::max(last.1, r.1);
                } else {
                    merged.push(*r);
                }
            } else {
                merged.push(*r);
            }
        }
        *ranges = merged;
    }

    files
}

/// Uses Tree-sitter to extract the highest-level meaningful enclosing block (like a function or struct)
/// for a given line range. Returns the source code of that block.
pub fn extract_enclosing_block(
    source_code: &str,
    start_line: usize,
    end_line: usize,
) -> Option<String> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_c::LANGUAGE.into()).ok()?;

    let tree = parser.parse(source_code, None)?;
    let root_node = tree.root_node();

    // Point uses row (0-based line) and column (0-based byte).
    // We want the narrowest descendant that completely covers our line range.
    let start_point = Point::new(start_line, 0);
    // Approximate end of line by using a large column value, or just 0 of next line.
    let end_point = Point::new(end_line, usize::MAX);

    let mut current_node = root_node.descendant_for_point_range(start_point, end_point)?;

    // We don't just want the exact line (which might be an `expression_statement` or `if_statement`).
    // We want the parent function, struct, or top-level declaration.
    let target_kinds = [
        "function_definition",
        "struct_specifier",
        "enum_specifier",
        "union_specifier",
        "declaration", // Top level declarations
        "type_definition",
    ];

    let mut found_block = None;

    loop {
        if target_kinds.contains(&current_node.kind()) {
            found_block = Some(current_node);
            // Don't break immediately; we might be inside a nested struct inside a function.
            // But usually, function_definition is good enough. Let's stick to the first match going up.
            break;
        }
        if let Some(parent) = current_node.parent() {
            current_node = parent;
        } else {
            break;
        }
    }

    // If we didn't find a top-level structure, fallback to a window of lines around the change
    if let Some(node) = found_block {
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        if start_byte < source_code.len() && end_byte <= source_code.len() {
            return Some(source_code[start_byte..end_byte].to_string());
        }
    }

    // Fallback: 20 lines around the change if it's completely outside any known block (e.g. macro definition)
    let lines: Vec<&str> = source_code.lines().collect();
    let start = start_line.saturating_sub(20);
    let end = std::cmp::min(lines.len().saturating_sub(1), end_line + 20);
    if start <= end && start < lines.len() {
        Some(lines[start..=end].join("\n"))
    } else {
        None
    }
}

fn is_common_c_word(word: &str) -> bool {
    let common = [
        "int", "char", "void", "long", "short", "unsigned", "signed", "struct", "union", "enum",
        "typedef", "static", "const", "volatile", "if", "else", "for", "while", "do", "switch",
        "case", "default", "return", "break", "continue", "goto", "sizeof", "true", "false",
        "NULL", "inline", "extern", "register", "auto", "restrict", "u8", "u16", "u32", "u64",
        "s8", "s16", "s32", "s64", "uint8_t", "uint16_t", "uint32_t", "uint64_t", "int8_t",
        "int16_t", "int32_t", "int64_t", "bool", "size_t", "ssize_t", "pid_t", "uid_t", "gid_t",
        "off_t", "ret", "err", "len", "size", "res", "tmp", "val", "ptr", "idx", "out",
    ];
    common.contains(&word)
}

pub fn extract_identifiers(
    source_code: &str,
    start_line: usize,
    end_line: usize,
) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .is_err()
    {
        return ids;
    }

    let tree = if let Some(t) = parser.parse(source_code, None) {
        t
    } else {
        return ids;
    };
    let root_node = tree.root_node();
    let start_point = Point::new(start_line, 0);
    let end_point = Point::new(end_line, usize::MAX);

    if let Some(node) = root_node.descendant_for_point_range(start_point, end_point) {
        fn walk<'a>(n: tree_sitter::Node<'a>, src: &[u8], out: &mut HashSet<String>) {
            let kind = n.kind();
            if (kind == "identifier" || kind == "type_identifier")
                && let Ok(text) = n.utf8_text(src)
            {
                let s = text.to_string();
                if s.len() >= 3 && !is_common_c_word(&s) {
                    out.insert(s);
                }
            }
            let mut cursor = n.walk();
            for child in n.children(&mut cursor) {
                walk(child, src, out);
            }
        }
        walk(node, source_code.as_bytes(), &mut ids);
    }
    ids
}

pub async fn prefetch_context(worktree_path: &Path, diff: &str) -> Result<(String, String)> {
    let mut context_blocks = Vec::new();
    let file_ranges = parse_diff_ranges(diff);

    let log_data: Vec<PrefetchFileLog> = file_ranges
        .iter()
        .map(|(filename, ranges)| PrefetchFileLog {
            filename: filename.clone(),
            ranges: ranges
                .iter()
                .map(|&(start, end)| PrefetchRangeLog {
                    start_line: start,
                    end_line: end,
                })
                .collect(),
        })
        .collect();

    let summary_json = serde_json::to_string_pretty(&log_data).unwrap_or_default();
    info!("Prefetching context for ranges: {}", summary_json);

    let mut symbols_to_lookup = HashSet::new();

    for (file, ranges) in file_ranges {
        if !file.ends_with(".c") && !file.ends_with(".h") {
            continue; // Only C/C++ files for tree-sitter-c
        }

        let file_path = worktree_path.join(&file);
        if !file_path.exists() {
            continue;
        }

        if let Ok(content) = fs::read_to_string(&file_path).await {
            // We use a HashSet (or sort/dedup) to avoid repeating the exact same block
            // if multiple hunks fall in the same function.
            let mut extracted_blocks = HashSet::new();

            for (start, end) in ranges {
                if let Some(block) = extract_enclosing_block(&content, start, end) {
                    extracted_blocks.insert(block);
                }
                let ids = extract_identifiers(&content, start, end);
                symbols_to_lookup.extend(ids);
            }

            for block in extracted_blocks {
                context_blocks.push(format!(
                    "--- Extracted Context from {} ---\n{}\n",
                    file, block
                ));
            }
        }
    }

    let mut definitions = Vec::new();
    let symbols: Vec<String> = symbols_to_lookup.into_iter().take(15).collect();
    for symbol in symbols {
        let regex = format!(
            "^(struct|enum|union)\\s+{0}\\b|^#define\\s+{0}\\b|^([a-zA-Z_][a-zA-Z0-9_ \\t*]+\\s+)?{0}\\s*\\(",
            symbol
        );
        if let Ok(output) = tokio::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["grep", "-n", "-E", &regex])
            .output()
            .await
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(first_line) = stdout.lines().next() {
                let parts: Vec<&str> = first_line.splitn(3, ':').collect();
                if parts.len() >= 2 {
                    let filename = parts[0];
                    if let Ok(line_num) = parts[1].parse::<usize>() {
                        let file_path = worktree_path.join(filename);
                        if let Ok(file_content) = fs::read_to_string(file_path).await {
                            let line_0 = line_num.saturating_sub(1);
                            if let Some(block) =
                                extract_enclosing_block(&file_content, line_0, line_0)
                            {
                                definitions.push(format!(
                                    "--- Extracted Definition of {} from {} ---
{}
",
                                    symbol, filename, block
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    context_blocks.extend(definitions);

    Ok((context_blocks.join("\n"), summary_json))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diff_ranges() {
        let diff = r#"
--- a/file.c
+++ b/file.c
@@ -10,2 +10,4 @@
 context
+new line 1
+new line 2
 context
@@ -50,0 +52,1 @@
+new line 3
"#;
        let ranges = parse_diff_ranges(diff);
        assert_eq!(ranges.len(), 1);
        let file_ranges = ranges.get("file.c").unwrap();
        assert_eq!(file_ranges.len(), 2);
        assert_eq!(file_ranges[0], (9, 12)); // 0-based: 10->9, count 4 -> 9,10,11,12 -> end 12
        assert_eq!(file_ranges[1], (51, 51)); // 0-based: 52->51, count 1 -> 51
    }

    #[test]
    fn test_extract_enclosing_block() {
        let source_code = r#"#include <stdio.h>

int main() {
    int a = 1;
    // target line 4 (0-based)
    printf("hello");
    return 0;
}

struct MyStruct {
    int x;
};
"#;
        let block_main = extract_enclosing_block(source_code, 4, 4).unwrap();
        assert!(block_main.starts_with("int main() {"));
        assert!(block_main.ends_with("return 0;\n}"));

        let block_struct = extract_enclosing_block(source_code, 10, 10).unwrap();
        assert!(block_struct.starts_with("struct MyStruct"));
    }
}
