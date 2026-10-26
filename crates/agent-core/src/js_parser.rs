//! JavaScript code parser - 使用 tree-sitter-javascript 进行 AST 级解析

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tree_sitter::Parser;

/// JS 语法错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSSyntaxError {
    pub message: String,
    pub line: Option<usize>,
    pub col: Option<usize>,
    pub end_line: Option<usize>,
    pub end_col: Option<usize>,
}

/// JS 导入信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSImport {
    pub kind: String, // "import", "require", or "dynamic_import"
    pub module: String,
    pub names: Vec<String>,
    pub asname: Option<String>,
    pub line: Option<usize>,
}

/// JS 函数定义信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSFunction {
    pub name: String,
    pub line: usize,
    pub parameters: Vec<String>,
}

/// JS 解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSParseResult {
    pub ok: bool,
    pub imports: Vec<JSImport>,
    pub functions: Vec<JSFunction>,
    pub modules: Vec<String>,
    pub has_module_syntax: bool,
    pub has_commonjs_syntax: bool,
    pub has_dynamic_import: bool,
    pub errors: Vec<JSSyntaxError>,
}

/// 分析 JavaScript 代码
pub fn analyze_javascript(code: &str) -> JSParseResult {
    let src = code.trim();
    
    if src.is_empty() {
        return JSParseResult {
            ok: false,
            imports: vec![],
            functions: vec![],
            modules: vec![],
            has_module_syntax: false,
            has_commonjs_syntax: false,
            has_dynamic_import: false,
            errors: vec![JSSyntaxError {
                message: "Empty JavaScript code".to_string(),
                line: None,
                col: None,
                end_line: None,
                end_col: None,
            }],
        };
    }
    
    let mut parser = Parser::new();
    
    // 设置 JavaScript 语言
    if let Err(e) = parser.set_language(&tree_sitter_javascript::LANGUAGE.into()) {
        return JSParseResult {
            ok: false,
            imports: vec![],
            functions: vec![],
            modules: vec![],
            has_module_syntax: false,
            has_commonjs_syntax: false,
            has_dynamic_import: false,
            errors: vec![JSSyntaxError {
                message: format!("Failed to load JavaScript parser: {}", e),
                line: None,
                col: None,
                end_line: None,
                end_col: None,
            }],
        };
    }
    
    // 解析代码
    let tree = match parser.parse(src, None) {
        Some(tree) => tree,
        None => {
            return JSParseResult {
                ok: false,
                imports: vec![],
                functions: vec![],
                modules: vec![],
                has_module_syntax: false,
                has_commonjs_syntax: false,
                has_dynamic_import: false,
                errors: vec![JSSyntaxError {
                    message: "Failed to parse JavaScript code".to_string(),
                    line: None,
                    col: None,
                    end_line: None,
                    end_col: None,
                }],
            };
        }
    };
    
    // 检查是否有语法错误
    let root = tree.root_node();
    if root.has_error() {
        let errors = extract_syntax_errors(&root, src);
        return JSParseResult {
            ok: false,
            imports: vec![],
            functions: vec![],
            modules: vec![],
            has_module_syntax: false,
            has_commonjs_syntax: false,
            has_dynamic_import: false,
            errors,
        };
    }
    
    // 提取导入和函数
    let mut imports = vec![];
    let mut functions = vec![];
    let mut modules_set = HashSet::new();
    let mut has_module_syntax = false;
    let mut has_commonjs_syntax = false;
    let mut has_dynamic_import = false;
    
    extract_imports(&root, src, &mut imports, &mut modules_set, 
                   &mut has_module_syntax, &mut has_commonjs_syntax, 
                   &mut has_dynamic_import);
    
    extract_functions(&root, src, &mut functions);
    
    let modules: Vec<String> = modules_set.into_iter().collect();
    
    JSParseResult {
        ok: true,
        imports,
        functions,
        modules,
        has_module_syntax,
        has_commonjs_syntax,
        has_dynamic_import,
        errors: vec![],
    }
}

/// 提取语法错误
fn extract_syntax_errors(node: &tree_sitter::Node, src: &str) -> Vec<JSSyntaxError> {
    let mut errors = vec![];
    
    if node.is_error() || node.is_missing() {
        let start = node.start_position();
        let end = node.end_position();
        errors.push(JSSyntaxError {
            message: if node.is_missing() {
                format!("Missing syntax element")
            } else {
                format!("Syntax error")
            },
            line: Some(start.row + 1),
            col: Some(start.column + 1),
            end_line: Some(end.row + 1),
            end_col: Some(end.column + 1),
        });
    }
    
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        errors.extend(extract_syntax_errors(&child, src));
    }
    
    errors
}

/// 提取导入语句
fn extract_imports(
    node: &tree_sitter::Node,
    src: &str,
    imports: &mut Vec<JSImport>,
    modules_set: &mut HashSet<String>,
    has_module_syntax: &mut bool,
    has_commonjs_syntax: &mut bool,
    has_dynamic_import: &mut bool,
) {
    match node.kind() {
        // ES6 import: import X from 'module'
        "import_statement" => {
            *has_module_syntax = true;
            extract_es6_import(node, src, imports, modules_set);
        }
        // require() call
        "call_expression" => {
            if let Some(func) = node.child_by_field_name("function") {
                if func.kind() == "identifier" {
                    let func_name = func.utf8_text(src.as_bytes()).unwrap_or("");
                    if func_name == "require" {
                        *has_commonjs_syntax = true;
                        extract_require_call(node, src, imports, modules_set);
                    }
                }
            }
        }
        // dynamic import: import('module')
        "import" => {
            if node.parent().map(|p| p.kind() == "call_expression").unwrap_or(false) {
                *has_dynamic_import = true;
                *has_module_syntax = true;
                extract_dynamic_import(node, src, imports, modules_set);
            }
        }
        _ => {}
    }
    
    // 递归处理子节点
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_imports(&child, src, imports, modules_set, 
                       has_module_syntax, has_commonjs_syntax, has_dynamic_import);
    }
}

/// 提取 ES6 import
fn extract_es6_import(
    node: &tree_sitter::Node,
    src: &str,
    imports: &mut Vec<JSImport>,
    modules_set: &mut HashSet<String>,
) {
    let mut module_name = String::new();
    let mut names = vec![];
    let mut asname = None;
    
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "string" => {
                // 提取字符串内容（去掉引号）
                let text = child.utf8_text(src.as_bytes()).unwrap_or("");
                if text.len() >= 2 {
                    module_name = text[1..text.len()-1].to_string();
                }
            }
            "identifier" => {
                let name = child.utf8_text(src.as_bytes()).unwrap_or("").to_string();
                names.push(name.clone());
                asname = Some(name);
            }
            "named_imports" | "namespace_import" => {
                // 提取导入的名称
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "identifier" {
                        let name = inner.utf8_text(src.as_bytes()).unwrap_or("").to_string();
                        names.push(name);
                    }
                }
            }
            _ => {}
        }
    }
    
    if !module_name.is_empty() {
        modules_set.insert(module_name.clone());
        imports.push(JSImport {
            kind: "import".to_string(),
            module: module_name,
            names,
            asname,
            line: Some(node.start_position().row + 1),
        });
    }
}

/// 提取 require 调用
fn extract_require_call(
    node: &tree_sitter::Node,
    src: &str,
    imports: &mut Vec<JSImport>,
    modules_set: &mut HashSet<String>,
) {
    if let Some(args) = node.child_by_field_name("arguments") {
        let mut cursor = args.walk();
        for child in args.children(&mut cursor) {
            if child.kind() == "string" {
                let text = child.utf8_text(src.as_bytes()).unwrap_or("");
                if text.len() >= 2 {
                    let module_name = text[1..text.len()-1].to_string();
                    modules_set.insert(module_name.clone());
                    imports.push(JSImport {
                        kind: "require".to_string(),
                        module: module_name,
                        names: vec![],
                        asname: None,
                        line: Some(node.start_position().row + 1),
                    });
                }
            }
        }
    }
}

/// 提取动态 import
fn extract_dynamic_import(
    node: &tree_sitter::Node,
    src: &str,
    imports: &mut Vec<JSImport>,
    modules_set: &mut HashSet<String>,
) {
    if let Some(parent) = node.parent() {
        if let Some(args) = parent.child_by_field_name("arguments") {
            let mut cursor = args.walk();
            for child in args.children(&mut cursor) {
                if child.kind() == "string" {
                    let text = child.utf8_text(src.as_bytes()).unwrap_or("");
                    if text.len() >= 2 {
                        let module_name = text[1..text.len()-1].to_string();
                        modules_set.insert(module_name.clone());
                        imports.push(JSImport {
                            kind: "dynamic_import".to_string(),
                            module: module_name,
                            names: vec![],
                            asname: None,
                            line: Some(node.start_position().row + 1),
                        });
                    }
                }
            }
        }
    }
}

/// 提取函数定义
fn extract_functions(node: &tree_sitter::Node, src: &str, functions: &mut Vec<JSFunction>) {
    match node.kind() {
        "function_declaration" | "function" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(src.as_bytes()).unwrap_or("").to_string();
                let line = name_node.start_position().row + 1;
                
                let mut parameters = vec![];
                if let Some(params) = node.child_by_field_name("parameters") {
                    let mut cursor = params.walk();
                    for param in params.children(&mut cursor) {
                        if param.kind() == "identifier" {
                            let param_name = param.utf8_text(src.as_bytes()).unwrap_or("").to_string();
                            parameters.push(param_name);
                        }
                    }
                }
                
                functions.push(JSFunction {
                    name,
                    line,
                    parameters,
                });
            }
        }
        // 箭头函数和函数表达式
        "variable_declarator" => {
            if let Some(value) = node.child_by_field_name("value") {
                if value.kind() == "arrow_function" || value.kind() == "function" {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = name_node.utf8_text(src.as_bytes()).unwrap_or("").to_string();
                        let line = name_node.start_position().row + 1;
                        
                        let mut parameters = vec![];
                        if let Some(params) = value.child_by_field_name("parameters") {
                            let mut cursor = params.walk();
                            for param in params.children(&mut cursor) {
                                if param.kind() == "identifier" {
                                    let param_name = param.utf8_text(src.as_bytes()).unwrap_or("").to_string();
                                    parameters.push(param_name);
                                }
                            }
                        }
                        
                        functions.push(JSFunction {
                            name,
                            line,
                            parameters,
                        });
                    }
                }
            }
        }
        _ => {}
    }
    
    // 递归处理子节点
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_functions(&child, src, functions);
    }
}

/// 检查危险的 Node.js 模块
pub fn check_dangerous_imports(code: &str) -> Vec<String> {
    let dangerous_modules = [
        "fs", "child_process", "exec", "eval", 
        "vm", "net", "dgram", "dns",
    ];
    
    let result = analyze_javascript(code);
    result.imports.into_iter()
        .filter(|imp| dangerous_modules.contains(&imp.module.as_str()))
        .map(|imp| imp.module)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_es6_import() {
        let code = "import React from 'react';";
        let result = analyze_javascript(code);
        assert!(result.ok);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].module, "react");
        assert!(result.has_module_syntax);
    }
    
    #[test]
    fn test_require() {
        let code = "const fs = require('fs');";
        let result = analyze_javascript(code);
        assert!(result.ok);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].module, "fs");
        assert!(result.has_commonjs_syntax);
    }
    
    #[test]
    fn test_dangerous_imports() {
        let code = "import fs from 'fs';\nconst exec = require('child_process');";
        let dangerous = check_dangerous_imports(code);
        assert!(dangerous.contains(&"fs".to_string()));
        assert!(dangerous.contains(&"child_process".to_string()));
    }
}
