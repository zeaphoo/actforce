//! Python 代码解析器 - 使用 tree-sitter-python 进行 AST 级解析

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tree_sitter::Parser;

/// Python 语法错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonSyntaxError {
    pub message: String,
    pub line: Option<usize>,
    pub col: Option<usize>,
    pub end_line: Option<usize>,
    pub end_col: Option<usize>,
}

/// Python 导入信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonImport {
    pub kind: ImportKind,
    pub module: Option<String>,
    pub name: String,
    pub alias: Option<String>,
    pub level: usize,
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportKind {
    Import,
    From,
}

/// Python 函数定义信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonFunction {
    pub name: String,
    pub line: usize,
    pub parameters: Vec<String>,
}

/// Python 类定义信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonClass {
    pub name: String,
    pub line: usize,
    pub bases: Vec<String>,
}

/// Python 解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonParseResult {
    pub ok: bool,
    pub imports: Vec<PythonImport>,
    pub modules: Vec<String>,
    pub stdlib_modules: Vec<String>,
    pub third_party_modules: Vec<String>,
    pub functions: Vec<PythonFunction>,
    pub classes: Vec<PythonClass>,
    pub errors: Vec<PythonSyntaxError>,
}

fn node_text(src: &[u8], node: &tree_sitter::Node) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    if start >= end {
        return String::new();
    }
    String::from_utf8_lossy(&src[start..end]).to_string()
}

/// 解析并分析 Python 代码（基于 tree-sitter AST）
pub fn analyze_python(code: &str) -> PythonParseResult {
    let src = code.trim();
    if src.is_empty() {
        return PythonParseResult {
            ok: false,
            imports: Vec::new(),
            modules: Vec::new(),
            stdlib_modules: Vec::new(),
            third_party_modules: Vec::new(),
            functions: Vec::new(),
            classes: Vec::new(),
            errors: vec![PythonSyntaxError {
                message: "Empty Python code".to_string(),
                line: None,
                col: None,
                end_line: None,
                end_col: None,
            }],
        };
    }

    let mut errors = Vec::new();
    let src_bytes = src.as_bytes();

    let mut parser = Parser::new();
    if let Err(e) = parser.set_language(&tree_sitter_python::LANGUAGE.into()) {
        return PythonParseResult {
            ok: false,
            imports: Vec::new(),
            modules: Vec::new(),
            stdlib_modules: Vec::new(),
            third_party_modules: Vec::new(),
            functions: Vec::new(),
            classes: Vec::new(),
            errors: vec![PythonSyntaxError {
                message: format!("tree-sitter-python language load failed: {}", e),
                line: None,
                col: None,
                end_line: None,
                end_col: None,
            }],
        };
    }

    let tree = match parser.parse(src, None) {
        Some(t) => t,
        None => {
            return PythonParseResult {
                ok: false,
                imports: Vec::new(),
                modules: Vec::new(),
                stdlib_modules: Vec::new(),
                third_party_modules: Vec::new(),
                functions: Vec::new(),
                classes: Vec::new(),
                errors: vec![PythonSyntaxError {
                    message: "tree-sitter parse failed".to_string(),
                    line: None,
                    col: None,
                    end_line: None,
                    end_col: None,
                }],
            };
        }
    };

    let root = tree.root_node();

    // 检测语法错误
    let mut ok = true;
    if root.has_error() {
        ok = false;
    }

    // 收集 ERROR 节点
    let mut error_nodes = Vec::new();
    fn find_error_nodes<'a>(node: &tree_sitter::Node<'a>, errors: &mut Vec<tree_sitter::Node<'a>>) {
        if node.kind() == "ERROR" || node.is_missing() {
            errors.push(node.clone());
        }
        let count = node.child_count();
        for i in 0..count {
            if let Some(child) = node.child(i as u32) {
                find_error_nodes(&child, errors);
            }
        }
    }
    find_error_nodes(&root, &mut error_nodes);

    for node in error_nodes {
        ok = false;
        let line = node.start_position().row + 1;
        let col = node.start_position().column;
        errors.push(PythonSyntaxError {
            message: "syntax error".to_string(),
            line: Some(line),
            col: Some(col),
            end_line: Some(node.end_position().row + 1),
            end_col: Some(node.end_position().column),
        });
    }

    // 提取导入（使用有序遍历）
    let mut imports = Vec::new();
    let mut modules_set = HashSet::new();

    let mut import_nodes = Vec::new();
    collect_nodes_by_type(
        &root,
        &["import_statement", "import_from_statement"],
        &mut import_nodes,
    );

    for node in import_nodes {
        let text = node_text(src_bytes, &node);
        match node.kind() {
            "import_statement" => {
                parse_import_node(&text, &node, &mut imports, &mut modules_set);
            }
            "import_from_statement" => {
                parse_from_import_node(&text, &node, &mut imports, &mut modules_set);
            }
            _ => {}
        }
    }

    let modules: Vec<String> = {
        let mut m: Vec<String> = modules_set.into_iter().collect();
        m.sort();
        m
    };

    // 分类标准库和第三方库
    let (stdlib_modules, third_party_modules) = classify_modules(&modules);

    // 提取函数和类
    let functions = extract_functions(src_bytes, &root);
    let classes = extract_classes(src_bytes, &root);

    PythonParseResult {
        ok,
        imports,
        modules,
        stdlib_modules,
        third_party_modules,
        functions,
        classes,
        errors,
    }
}

fn parse_import_node(
    text: &str,
    node: &tree_sitter::Node,
    imports: &mut Vec<PythonImport>,
    modules_set: &mut HashSet<String>,
) {
    // import os, sys
    // import numpy as np
    let line_num = node.start_position().row + 1;

    // 使用正则解析 import 语句
    let re = regex::Regex::new(r"^import\s+(.+)$").unwrap();
    if let Some(cap) = re.captures(text.trim()) {
        let body = cap.get(1).unwrap().as_str().trim();
        for part in body.split(',') {
            let part = part.trim();
            let (name, alias) = if part.contains(" as ") {
                let parts: Vec<&str> = part.splitn(2, " as ").collect();
                (
                    parts[0].trim().to_string(),
                    Some(parts[1].trim().to_string()),
                )
            } else {
                (part.to_string(), None)
            };
            if let Some(top) = name.split('.').next() {
                if !top.is_empty() {
                    modules_set.insert(top.to_string());
                }
            }
            imports.push(PythonImport {
                kind: ImportKind::Import,
                module: None,
                name,
                alias,
                level: 0,
                line: Some(line_num),
            });
        }
    }
}

fn parse_from_import_node(
    text: &str,
    node: &tree_sitter::Node,
    imports: &mut Vec<PythonImport>,
    modules_set: &mut HashSet<String>,
) {
    // from os import path
    // from .. import utils
    // from collections import defaultdict as dd
    // from collections import (
    //     defaultdict,
    //     OrderedDict,
    // )
    let line_num = node.start_position().row + 1;

    // 使用 (?s) 使 . 匹配换行符
    let re = regex::Regex::new(r"(?s)^from\s+(\.*)(\S*?)\s+import\s+(.+)$").unwrap();
    if let Some(cap) = re.captures(text.trim()) {
        let dots = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let level = dots.len();
        let module = cap.get(2).map(|m| m.as_str().to_string());
        let body = cap.get(3).unwrap().as_str().trim();

        // 移除括号（多行导入）
        let body = body
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim();

        if let Some(ref mod_name) = module {
            if level == 0 {
                if let Some(top) = mod_name.split('.').next() {
                    if !top.is_empty() {
                        modules_set.insert(top.to_string());
                    }
                }
            }
        }

        for part in body.split(',') {
            let part = part.trim().replace('\n', "").trim().to_string();
            if part.is_empty() || part == "*" {
                imports.push(PythonImport {
                    kind: ImportKind::From,
                    module: module.clone(),
                    name: part.to_string(),
                    alias: None,
                    level,
                    line: Some(line_num),
                });
                continue;
            }
            let (name, alias) = if part.contains(" as ") {
                let parts: Vec<&str> = part.splitn(2, " as ").collect();
                (
                    parts[0].trim().to_string(),
                    Some(parts[1].trim().to_string()),
                )
            } else {
                (part.to_string(), None)
            };
            imports.push(PythonImport {
                kind: ImportKind::From,
                module: module.clone(),
                name,
                alias,
                level,
                line: Some(line_num),
            });
        }
    }
}

fn classify_modules(modules: &[String]) -> (Vec<String>, Vec<String>) {
    // 使用 sys.stdlib_module_names 如果可用（Rust 中没有直接等价物）
    // 这里使用一个常见标准库列表
    const STDLIB_MODULES: &[&str] = &[
        "abc",
        "argparse",
        "array",
        "ast",
        "asyncio",
        "atexit",
        "base64",
        "bisect",
        "builtins",
        "calendar",
        "collections",
        "concurrent",
        "contextlib",
        "copy",
        "csv",
        "ctypes",
        "dataclasses",
        "datetime",
        "decimal",
        "difflib",
        "dis",
        "enum",
        "errno",
        "faulthandler",
        "fcntl",
        "filecmp",
        "fileinput",
        "fnmatch",
        "functools",
        "gc",
        "getopt",
        "getpass",
        "gettext",
        "glob",
        "graphlib",
        "gzip",
        "hashlib",
        "heapq",
        "hmac",
        "html",
        "http",
        "idlelib",
        "imaplib",
        "imghdr",
        "imp",
        "importlib",
        "inspect",
        "io",
        "ipaddress",
        "itertools",
        "json",
        "keyword",
        "lib2to3",
        "linecache",
        "locale",
        "logging",
        "lzma",
        "mailbox",
        "mailcap",
        "marshal",
        "math",
        "mimetypes",
        "mmap",
        "modulefinder",
        "multiprocessing",
        "netrc",
        "nis",
        "nntplib",
        "numbers",
        "operator",
        "optparse",
        "os",
        "ossaudiodev",
        "pathlib",
        "pdb",
        "pickle",
        "pickletools",
        "pipes",
        "pkgutil",
        "platform",
        "plistlib",
        "poplib",
        "posix",
        "posixpath",
        "pprint",
        "profile",
        "pstats",
        "pty",
        "pwd",
        "py_compile",
        "pyclbr",
        "pydoc",
        "queue",
        "quopri",
        "random",
        "re",
        "readline",
        "reprlib",
        "resource",
        "rlcompleter",
        "runpy",
        "sched",
        "secrets",
        "select",
        "selectors",
        "shelve",
        "shlex",
        "shutil",
        "signal",
        "site",
        "smtpd",
        "smtplib",
        "sndhdr",
        "socket",
        "socketserver",
        "spwd",
        "sqlite3",
        "ssl",
        "stat",
        "statistics",
        "string",
        "stringprep",
        "struct",
        "subprocess",
        "sunau",
        "symtable",
        "sys",
        "sysconfig",
        "syslog",
        "tabnanny",
        "tarfile",
        "telnetlib",
        "tempfile",
        "termios",
        "test",
        "textwrap",
        "threading",
        "time",
        "timeit",
        "tkinter",
        "token",
        "tokenize",
        "trace",
        "traceback",
        "tracemalloc",
        "tty",
        "turtle",
        "turtledemo",
        "types",
        "typing",
        "unicodedata",
        "unittest",
        "urllib",
        "uu",
        "uuid",
        "venv",
        "warnings",
        "wave",
        "weakref",
        "webbrowser",
        "winreg",
        "winsound",
        "wsgiref",
        "xdrlib",
        "xml",
        "xmlrpc",
        "zipapp",
        "zipfile",
        "zipimport",
        "zlib",
    ];

    let stdlib_set: HashSet<&str> = STDLIB_MODULES.iter().cloned().collect();

    let mut stdlib = Vec::new();
    let mut third_party = Vec::new();

    for m in modules {
        let top = m.split('.').next().unwrap_or(m);
        if stdlib_set.contains(top) {
            stdlib.push(m.clone());
        } else {
            third_party.push(m.clone());
        }
    }

    (stdlib, third_party)
}

/// 递归遍历 AST 树，按文档顺序收集节点
fn collect_nodes_by_type<'a>(
    node: &tree_sitter::Node<'a>,
    target_types: &[&str],
    results: &mut Vec<tree_sitter::Node<'a>>,
) {
    let node_type = node.kind();
    if target_types.contains(&node_type) {
        results.push(node.clone());
    }
    // 按顺序遍历子节点
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            collect_nodes_by_type(&child, target_types, results);
        }
    }
}

fn extract_functions(src: &[u8], root: &tree_sitter::Node) -> Vec<PythonFunction> {
    let mut func_nodes = Vec::new();
    collect_nodes_by_type(root, &["function_definition"], &mut func_nodes);

    let mut functions = Vec::new();
    for node in func_nodes {
        let text = node_text(src, &node);
        let line = node.start_position().row + 1;

        // 提取函数名
        let name_re = regex::Regex::new(r"^def\s+(\w+)\s*\(").unwrap();
        if let Some(cap) = name_re.captures(text.trim()) {
            let name = cap.get(1).unwrap().as_str().to_string();

            // 提取参数 - 需要处理多行参数
            // 查找从 '(' 到匹配 ')' 的完整内容
            let params_str = extract_parentheses_content(&text);
            let parameters = if !params_str.is_empty() {
                parse_parameters(&params_str)
            } else {
                Vec::new()
            };

            functions.push(PythonFunction {
                name,
                line,
                parameters,
            });
        }
    }

    functions
}

fn extract_classes(src: &[u8], root: &tree_sitter::Node) -> Vec<PythonClass> {
    let mut class_nodes = Vec::new();
    collect_nodes_by_type(root, &["class_definition"], &mut class_nodes);

    let mut classes = Vec::new();
    for node in class_nodes {
        let text = node_text(src, &node);
        let line = node.start_position().row + 1;

        // 提取类名和基类
        let class_re = regex::Regex::new(r"^class\s+(\w+)(?:\(([^)]*)\))?").unwrap();
        if let Some(cap) = class_re.captures(text.trim()) {
            let name = cap.get(1).unwrap().as_str().to_string();
            let bases_str = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let bases = if bases_str.is_empty() {
                Vec::new()
            } else {
                bases_str
                    .split(',')
                    .map(|b| b.trim().to_string())
                    .filter(|b| !b.is_empty())
                    .collect()
            };

            classes.push(PythonClass { name, line, bases });
        }
    }

    classes
}

/// 提取括号内的完整内容（处理嵌套括号）
fn extract_parentheses_content(text: &str) -> String {
    let start = match text.find('(') {
        Some(pos) => pos,
        None => return String::new(),
    };

    let mut depth = 0;
    let mut end = start;
    let chars: Vec<char> = text.chars().collect();
    let mut found = false;

    for (i, &ch) in chars.iter().enumerate().skip(start) {
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                end = i;
                found = true;
                break;
            }
        }
    }

    if !found {
        // 没有闭合括号（可能是语法错误），返回剩余内容
        return text[start + 1..].to_string();
    }

    text[start + 1..end].to_string()
}

/// 解析参数字符串，处理默认值和类型注解
fn parse_parameters(params_str: &str) -> Vec<String> {
    if params_str.trim().is_empty() {
        return Vec::new();
    }

    // 处理跨行参数：先清理换行和多余空格
    let cleaned = params_str.replace('\n', " ").trim().to_string();

    let mut params = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for ch in cleaned.chars() {
        match ch {
            ',' if depth == 0 => {
                let p = current.trim().to_string();
                if !p.is_empty() && p != "self" && p != "cls" {
                    // 移除默认值
                    let p = if let Some(pos) = p.find('=') {
                        p[..pos].trim().to_string()
                    } else {
                        p
                    };
                    if !p.is_empty() {
                        params.push(p);
                    }
                }
                current.clear();
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            _ => {
                current.push(ch);
            }
        }
    }

    // 处理最后一个参数
    if !current.trim().is_empty() {
        let p = current.trim().to_string();
        if p != "self" && p != "cls" {
            let p = if let Some(pos) = p.find('=') {
                p[..pos].trim().to_string()
            } else {
                p
            };
            if !p.is_empty() {
                params.push(p);
            }
        }
    }

    params
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_statement() {
        let code = "import os\nimport sys\nimport numpy as np";
        let result = analyze_python(code);

        assert!(result.ok);
        assert_eq!(result.imports.len(), 3);
        assert!(result.stdlib_modules.contains(&"os".to_string()));
        assert!(result.stdlib_modules.contains(&"sys".to_string()));
        assert!(result.third_party_modules.contains(&"numpy".to_string()));
    }

    #[test]
    fn test_from_import() {
        let code = "from os import path\nfrom collections import defaultdict\nfrom . import utils";
        let result = analyze_python(code);

        assert!(result.ok);
        assert!(result
            .imports
            .iter()
            .any(|i| i.module == Some("os".to_string())));
        assert!(result
            .imports
            .iter()
            .any(|i| i.module == Some("collections".to_string())));
    }

    #[test]
    fn test_extract_functions() {
        let code = "def hello():\n    pass\n\ndef world(x, y=10):\n    return x";
        let result = analyze_python(code);

        assert_eq!(result.functions.len(), 2);
        assert_eq!(result.functions[0].name, "hello");
        assert_eq!(result.functions[1].name, "world");
        assert_eq!(result.functions[1].parameters.len(), 2); // x, y
    }

    #[test]
    fn test_extract_classes() {
        let code = "class Foo:\n    pass\n\nclass Bar(Base):\n    pass";
        let result = analyze_python(code);

        assert_eq!(result.classes.len(), 2);
        assert_eq!(result.classes[0].name, "Foo");
        assert_eq!(result.classes[1].name, "Bar");
        assert_eq!(result.classes[1].bases, vec!["Base"]);
    }

    #[test]
    fn test_syntax_error() {
        let code = "def foo(:\n    pass";
        let result = analyze_python(code);

        assert!(!result.ok);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_empty_code() {
        let result = analyze_python("");
        assert!(!result.ok);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_multi_line_import() {
        let code = "from collections import (\n    defaultdict,\n    OrderedDict,\n)";
        let result = analyze_python(code);

        assert!(result.ok);
        assert!(result.imports.iter().any(|i| i.name == "defaultdict"));
        assert!(result.imports.iter().any(|i| i.name == "OrderedDict"));
    }

    #[test]
    fn test_dangerous_imports_aligned_with_agentlite() {
        // 普通 import：模块为 None，须命中顶层模块
        assert!(check_dangerous_imports("import os").contains(&"os".to_string()));
        // 子路径 import：import os.path 命中 os
        assert!(check_dangerous_imports("import os.path").contains(&"os".to_string()));
        // from 导入：from http.client import HTTPConnection 命中 http
        assert!(check_dangerous_imports("from http.client import HTTPConnection").contains(&"http".to_string()));
        // 子路径 from 导入：from urllib.request import urlopen 命中 urllib
        assert!(check_dangerous_imports("from urllib.request import urlopen").contains(&"urllib".to_string()));
        // 多模块 import：import sys, subprocess 命中 sys/subprocess
        let found = check_dangerous_imports("import sys, subprocess");
        assert!(found.contains(&"sys".to_string()));
        assert!(found.contains(&"subprocess".to_string()));
        // 危险列表对齐 agentlite：pickle/marshal/importlib 不再命中
        assert!(check_dangerous_imports("import pickle").is_empty());
        assert!(check_dangerous_imports("import importlib").is_empty());
        // 普通安全模块不命中
        assert!(check_dangerous_imports("import json, math").is_empty());
    }
}

/// 检查危险的 Python 模块（对齐 agentlite python_parser.py:207-231）
///
/// 按解析结果的顶层模块匹配：`analyze_python` 的 `modules` 已同时覆盖
/// 普通 import（`import os`、`import os.path` → `os`）与 from 导入
/// （`from http.client import X` → `http`），与 agentlite 的
/// `for module in result.modules` 语义一致。
pub fn check_dangerous_imports(code: &str) -> Vec<String> {
    let dangerous_modules = [
        "os", "subprocess", "shutil", "sys",
        "socket", "http", "urllib", "ctypes", "multiprocessing",
    ];

    let result = analyze_python(code);
    result.modules.into_iter()
        .filter(|module| dangerous_modules.contains(&module.as_str()))
        .collect()
}
