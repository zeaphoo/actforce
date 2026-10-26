//! Shell 脚本解析器 - 使用 tree-sitter-bash 进行 AST 级解析

use serde::{Deserialize, Serialize};
use tree_sitter::Parser;

/// Shell 脚本复杂度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellScriptComplexity {
    Simple,
    Pipeline,
    Complex,
    Unknown,
}

/// Shell 发现严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellFindingSeverity {
    Low,
    Medium,
    High,
}

/// 命令位置信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellCommandSpan {
    pub start_byte: usize,
    pub end_byte: usize,
}

/// 简单命令信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellSimpleCommand {
    pub command: Option<String>,
    pub argv: Vec<String>,
    pub assignments: Vec<(String, Option<String>)>,
    pub has_redirect: bool,
    pub span: Option<ShellCommandSpan>,
}

/// Shell 命令分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellCommandAnalysis {
    pub complexity: ShellScriptComplexity,
    pub commands: Vec<ShellSimpleCommand>,
    pub main_commands: Vec<String>,
    pub has_pipe: bool,
    pub has_and_or: bool,
    pub has_subshell: bool,
    pub has_command_substitution: bool,
    pub has_redirect: bool,
    pub findings: Vec<ShellFinding>,
    pub errors: Vec<String>,
}

/// Shell 发现项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellFinding {
    pub severity: ShellFindingSeverity,
    pub rule_id: String,
    pub message: String,
    pub command: Option<String>,
    pub span: Option<ShellCommandSpan>,
}

fn node_text(src: &[u8], node: &tree_sitter::Node) -> String {
    src[node.start_byte()..node.end_byte()]
        .iter()
        .map(|&b| b as char)
        .collect()
}

fn has_node_type(root: &tree_sitter::Node, types: &[&str]) -> bool {
    let mut cursor = root.walk();
    let mut stack = vec![root.clone()];
    while let Some(node) = stack.pop() {
        let node_type = node.kind();
        if types.contains(&node_type) {
            return true;
        }
        node.children(&mut cursor).for_each(|child| {
            stack.push(child);
        });
    }
    false
}

fn extract_simple_commands(src: &[u8], root: &tree_sitter::Node) -> Vec<ShellSimpleCommand> {
    let mut commands = Vec::new();
    let mut seen_spans = std::collections::HashSet::new();

    // 遍历所有可能的命令节点
    let mut cursor = root.walk();
    let mut stack = vec![root.clone()];
    while let Some(node) = stack.pop() {
        let node_type = node.kind();
        // tree-sitter-bash 可能使用多种节点类型
        let is_command = matches!(
            node_type,
            "command" | "simple_command" | "command_name" | "program"
        );

        // 对于 leaf command_name 节点，提取其文本作为命令
        if node_type == "command_name" {
            let text = node_text(src, &node);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                node.children(&mut cursor).for_each(|child| {
                    stack.push(child);
                });
                continue;
            }

            let span = ShellCommandSpan {
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
            };
            let span_key = (span.start_byte, span.end_byte);
            if !seen_spans.contains(&span_key) {
                seen_spans.insert(span_key);
                commands.push(ShellSimpleCommand {
                    command: Some(trimmed.to_string()),
                    argv: vec![trimmed.to_string()],
                    assignments: Vec::new(),
                    has_redirect: false,
                    span: Some(span),
                });
            }
        } else if is_command {
            let text = node_text(src, &node);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                node.children(&mut cursor).for_each(|child| {
                    stack.push(child);
                });
                continue;
            }

            let span = ShellCommandSpan {
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
            };
            let span_key = (span.start_byte, span.end_byte);
            if seen_spans.contains(&span_key) {
                continue;
            }
            seen_spans.insert(span_key);

            // 检查是否有重定向
            let has_redirect = has_node_type(
                &node,
                &[
                    "redirected_statement",
                    "redirect",
                    "file_redirect",
                    "heredoc_redirect",
                ],
            );

            // 启发式解析：分割为赋值和命令参数
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            let mut assignments: Vec<(String, Option<String>)> = Vec::new();
            let mut argv_start = 0;

            // 识别 KEY=VAL 赋值
            for (i, p) in parts.iter().enumerate() {
                if let Some(eq_pos) = p.find('=') {
                    if eq_pos > 0 {
                        let key = &p[..eq_pos];
                        if key.chars().all(|c| c.is_alphanumeric() || c == '_')
                            && key
                                .chars()
                                .next()
                                .map(|c| c.is_alphabetic() || c == '_')
                                .unwrap_or(false)
                        {
                            let val = if eq_pos + 1 < p.len() {
                                Some(p[eq_pos + 1..].to_string())
                            } else {
                                Some(String::new())
                            };
                            assignments.push((key.to_string(), val));
                            argv_start = i + 1;
                            continue;
                        }
                    }
                }
                break;
            }

            let argv: Vec<String> = parts[argv_start..].iter().map(|s| s.to_string()).collect();

            let command = if argv.is_empty() {
                None
            } else {
                let mut cmd = &argv[0];
                // 处理 sudo 前缀
                if cmd == "sudo" {
                    for arg in argv.iter().skip(1) {
                        if !arg.starts_with('-') {
                            cmd = arg;
                            break;
                        }
                    }
                }
                Some(cmd.to_string())
            };

            commands.push(ShellSimpleCommand {
                command,
                argv,
                assignments,
                has_redirect,
                span: Some(span),
            });
        }

        node.children(&mut cursor).for_each(|child| {
            stack.push(child);
        });
    }

    commands
}

fn check_rm_rf(argv: &[String]) -> bool {
    if argv.is_empty() || argv[0] != "rm" {
        return false;
    }
    let mut has_r = false;
    let mut has_f = false;
    for a in argv.iter().skip(1) {
        if !a.starts_with('-') {
            continue;
        }
        match a.as_str() {
            "--recursive" => has_r = true,
            "--force" => has_f = true,
            _ if a.starts_with('-') && !a.starts_with("--") => {
                if a.contains('r') || a.contains('R') {
                    has_r = true;
                }
                if a.contains('f') {
                    has_f = true;
                }
            }
            _ => {}
        }
    }
    has_r && has_f
}

fn rm_targets(argv: &[String]) -> Vec<String> {
    argv.iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .cloned()
        .collect()
}

fn is_dangerous_path(p: &str) -> bool {
    matches!(p, "/" | "/*") || p.starts_with("/*")
}

/// 解析并分析 Bash 脚本（基于 tree-sitter AST）
pub fn analyze_bash(src: &str) -> ShellCommandAnalysis {
    let mut errors = Vec::new();
    let src_bytes = src.as_bytes();

    let mut parser = Parser::new();
    if let Err(e) = parser.set_language(&tree_sitter_bash::LANGUAGE.into()) {
        errors.push(format!("tree-sitter-bash language load failed: {}", e));
    }

    let tree = match parser.parse(src, None) {
        Some(t) => t,
        None => {
            errors.push("tree-sitter parse failed".to_string());
            return ShellCommandAnalysis {
                complexity: ShellScriptComplexity::Unknown,
                commands: vec![],
                main_commands: vec![],
                has_pipe: false,
                has_and_or: false,
                has_subshell: false,
                has_command_substitution: false,
                has_redirect: false,
                findings: vec![],
                errors,
            };
        }
    };

    let root = tree.root_node();

    // 检测 AST 级别的特性
    let has_pipe = has_node_type(&root, &["pipeline", "pipe_sequence"]) || src.contains('|');
    let has_and_or =
        has_node_type(&root, &["and_or", "list"]) || src.contains("&&") || src.contains("||");
    let has_redirect = has_node_type(
        &root,
        &[
            "redirect",
            "file_redirect",
            "heredoc_redirect",
            "redirected_statement",
        ],
    ) || src.contains('>')
        || src.contains('<');
    let has_subshell = has_node_type(&root, &["subshell", "subshell_expression"])
        || (src.contains('(') && src.contains(')'));
    let has_command_substitution =
        has_node_type(&root, &["command_substitution"]) || src.contains("$(") || src.contains('`');

    // 提取命令
    let commands = extract_simple_commands(src_bytes, &root);

    // 提取主命令
    let mut main_commands = Vec::new();
    let mut seen_cmds = std::collections::HashSet::new();
    for c in &commands {
        if let Some(ref cmd) = c.command {
            if seen_cmds.insert(cmd.clone()) {
                main_commands.push(cmd.clone());
            }
        }
    }

    // 安全检查
    let mut findings = Vec::new();

    // 规则: rm -rf
    for c in &commands {
        if check_rm_rf(&c.argv) {
            let targets = rm_targets(&c.argv);
            let severity = if targets.iter().any(|t| is_dangerous_path(t)) {
                ShellFindingSeverity::High
            } else {
                ShellFindingSeverity::Medium
            };
            findings.push(ShellFinding {
                severity,
                rule_id: "rm_rf".to_string(),
                message: "Detected 'rm -rf' which is potentially destructive.".to_string(),
                command: c.command.clone(),
                span: c.span.clone(),
            });
        }
    }

    // 规则: curl|bash / wget|sh
    if has_pipe {
        let downloaders = ["curl", "wget"];
        let shells = ["bash", "sh", "zsh", "dash", "ksh"];
        for (i, c) in commands.iter().enumerate() {
            if downloaders.contains(&c.command.as_deref().unwrap_or("")) {
                for later in commands.iter().skip(i + 1) {
                    if shells.contains(&later.command.as_deref().unwrap_or("")) {
                        findings.push(ShellFinding {
                            severity: ShellFindingSeverity::High,
                            rule_id: "download_pipe_shell".to_string(),
                            message: "Detected 'curl|sh' / 'wget|sh' pattern (downloaded content piped into a shell)."
                                .to_string(),
                            command: later.command.clone(),
                            span: later.span.clone(),
                        });
                        break;
                    }
                }
            }
        }
    }

    // 复杂度分类
    let complexity = if src.trim().is_empty() {
        ShellScriptComplexity::Unknown
    } else if has_pipe && !has_and_or && !has_subshell && !has_command_substitution {
        ShellScriptComplexity::Pipeline
    } else if !has_pipe && !has_and_or && !has_subshell && !has_command_substitution {
        ShellScriptComplexity::Simple
    } else {
        ShellScriptComplexity::Complex
    };

    ShellCommandAnalysis {
        complexity,
        commands,
        main_commands,
        has_pipe,
        has_and_or,
        has_subshell,
        has_command_substitution,
        has_redirect,
        findings,
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_command() {
        let result = analyze_bash("echo hello");
        assert_eq!(result.main_commands.len(), 1);
        assert_eq!(result.main_commands[0], "echo");
        assert_eq!(result.complexity, ShellScriptComplexity::Simple);
    }

    #[test]
    fn test_pipeline() {
        let result = analyze_bash("cat file.txt | grep pattern | wc -l");
        assert!(result.has_pipe);
        assert_eq!(result.complexity, ShellScriptComplexity::Pipeline);
    }

    #[test]
    fn test_and_or() {
        let result = analyze_bash("mkdir -p dir && cd dir");
        assert!(result.has_and_or);
        assert_eq!(result.complexity, ShellScriptComplexity::Complex);
    }

    #[test]
    fn test_rm_rf_detection() {
        let result = analyze_bash("rm -rf /tmp/test");
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].rule_id, "rm_rf");
    }

    #[test]
    fn test_curl_bash_detection() {
        let result = analyze_bash("curl https://example.com | bash");
        assert!(result.has_pipe);
        let has_download_finding = result
            .findings
            .iter()
            .any(|f| f.rule_id == "download_pipe_shell");
        assert!(has_download_finding);
    }

    #[test]
    fn test_subshell_detection() {
        let result = analyze_bash("(cd /tmp && ls)");
        assert!(result.has_subshell);
    }

    #[test]
    fn test_command_substitution_detection() {
        let result = analyze_bash("echo $(pwd)");
        assert!(result.has_command_substitution);
    }

    #[test]
    fn test_sudo_prefix() {
        let result = analyze_bash("sudo -E apt update");
        assert_eq!(result.main_commands[0], "apt");
    }
}
