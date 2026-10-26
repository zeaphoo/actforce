use std::collections::HashSet;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sqlparser::ast::{
    FromTable, Join, JoinOperator, Query, SetExpr, Statement, TableFactor, TableWithJoins,
};
use sqlparser::dialect::{GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlSyntaxError {
    pub message: String,
    /// 错误所在行（尽力而为：从 sqlparser 错误信息中提取，失败为 None）
    #[serde(default)]
    pub line: Option<usize>,
    /// 错误所在列（尽力而为）
    #[serde(default)]
    pub col: Option<usize>,
}

/// SQL 解析结果（对齐 agentlite sql_parser.py:17-25）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlParseResult {
    pub ok: bool,
    pub dialect: Option<String>,
    pub statement_count: usize,
    /// 各语句的原始文本
    #[serde(default)]
    pub statements: Vec<String>,
    /// 各语句的 AST 类型名（尽力而为）
    #[serde(default)]
    pub ast_types: Vec<String>,
    /// 规范化后的 SQL（仅 ok 时提供，空白归一化；对齐 agentlite 的 fallback 实现）
    #[serde(default)]
    pub normalized_sql: Option<String>,
    pub errors: Vec<SqlSyntaxError>,
}

fn dialect_from_name(name: Option<&str>) -> Box<dyn sqlparser::dialect::Dialect> {
    match name.unwrap_or("").to_lowercase().as_str() {
        "postgres" | "postgresql" => Box::new(PostgreSqlDialect {}),
        "mysql" => Box::new(MySqlDialect {}),
        "sqlite" => Box::new(SQLiteDialect {}),
        _ => Box::new(GenericDialect {}),
    }
}

/// 从 sqlparser 错误信息中尽力提取行/列（形如 "at Line: 3, Column 5"）
fn extract_error_line_col(msg: &str) -> (Option<usize>, Option<usize>) {
    let re = regex::Regex::new(r"at Line: (\d+), Column (\d+)").unwrap();
    if let Some(cap) = re.captures(msg) {
        let line = cap
            .get(1)
            .and_then(|m| m.as_str().parse::<usize>().ok());
        let col = cap
            .get(2)
            .and_then(|m| m.as_str().parse::<usize>().ok());
        (line, col)
    } else {
        (None, None)
    }
}

/// 空白归一化（对齐 agentlite fallback 的 `re.sub(r'\s+', ' ', stripped)`）
fn normalize_sql(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 语句的 AST 类型名（尽力而为，覆盖常用语句；其余回退到首关键字）
fn statement_ast_type(stmt: &Statement) -> String {
    match stmt {
        Statement::Query(_) => "Query".to_string(),
        Statement::Insert(_) => "Insert".to_string(),
        Statement::Update { .. } => "Update".to_string(),
        Statement::Delete(_) => "Delete".to_string(),
        Statement::CreateTable { .. } => "CreateTable".to_string(),
        Statement::CreateView { .. } => "CreateView".to_string(),
        Statement::CreateIndex { .. } => "CreateIndex".to_string(),
        Statement::CreateDatabase { .. } => "CreateDatabase".to_string(),
        Statement::AlterTable { .. } => "AlterTable".to_string(),
        Statement::Drop { .. } => "Drop".to_string(),
        Statement::Truncate { .. } => "Truncate".to_string(),
        Statement::Explain { .. } => "Explain".to_string(),
        Statement::SetVariable { .. } => "SetVariable".to_string(),
        Statement::ShowVariable { .. } => "ShowVariable".to_string(),
        Statement::Use { .. } => "Use".to_string(),
        Statement::StartTransaction { .. } => "StartTransaction".to_string(),
        Statement::Commit { .. } => "Commit".to_string(),
        Statement::Rollback { .. } => "Rollback".to_string(),
        Statement::Grant { .. } => "Grant".to_string(),
        Statement::Revoke { .. } => "Revoke".to_string(),
        _ => stmt
            .to_string()
            .split_whitespace()
            .next()
            .map(|w| w.to_string())
            .unwrap_or_else(|| "Statement".to_string()),
    }
}

pub fn validate_sql(sql: &str, dialect: Option<&str>) -> SqlParseResult {
    let raw = sql.trim();
    if raw.is_empty() {
        return SqlParseResult {
            ok: false,
            dialect: dialect.map(|s| s.to_string()),
            statement_count: 0,
            statements: Vec::new(),
            ast_types: Vec::new(),
            normalized_sql: None,
            errors: vec![SqlSyntaxError {
                message: "Empty SQL".to_string(),
                line: None,
                col: None,
            }],
        };
    }

    let d = dialect_from_name(dialect);
    match Parser::parse_sql(d.as_ref(), raw) {
        Ok(stmts) => SqlParseResult {
            ok: true,
            dialect: dialect.map(|s| s.to_string()),
            statement_count: stmts.len(),
            statements: stmts.iter().map(|s| s.to_string()).collect(),
            ast_types: stmts.iter().map(statement_ast_type).collect(),
            normalized_sql: Some(normalize_sql(raw)),
            errors: vec![],
        },
        Err(e) => {
            let msg = e.to_string();
            let (line, col) = extract_error_line_col(&msg);
            SqlParseResult {
                ok: false,
                dialect: dialect.map(|s| s.to_string()),
                statement_count: 0,
                statements: Vec::new(),
                ast_types: Vec::new(),
                normalized_sql: None,
                errors: vec![SqlSyntaxError {
                    message: msg,
                    line,
                    col,
                }],
            }
        }
    }
}

/// 提取物理表名。
///
/// 对齐 agentlite sql_parser.py extract_tables：
/// - 覆盖 SELECT（FROM/JOIN）与 DELETE FROM（含多表 DELETE / USING）
/// - 排除 CTE 名
/// - 递归进入子查询与 CTE 体内收集表
pub fn extract_tables(sql: &str, dialect: Option<&str>) -> Result<HashSet<String>> {
    let raw = sql.trim();
    if raw.is_empty() {
        return Ok(HashSet::new());
    }
    let d = dialect_from_name(dialect);
    let stmts = Parser::parse_sql(d.as_ref(), raw).map_err(|e| anyhow!(e.to_string()))?;

    let mut tables: HashSet<String> = HashSet::new();
    for stmt in &stmts {
        match stmt {
            Statement::Query(q) => collect_tables_from_query(q, &mut tables)?,
            Statement::Delete(del) => {
                // 多表 DELETE 的目标表（DELETE t1, t2 FROM ...）
                for t in &del.tables {
                    tables.insert(t.to_string());
                }
                let from_list = match &del.from {
                    FromTable::WithFromKeyword(v) => v,
                    FromTable::WithoutKeyword(v) => v,
                };
                for twj in from_list {
                    collect_tables_from_table_with_joins(twj, &mut tables)?;
                }
                // USING 子句（Snowflake/Postgres/MySQL）
                if let Some(using) = &del.using {
                    for twj in using {
                        collect_tables_from_table_with_joins(twj, &mut tables)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(tables)
}

fn collect_tables_from_query(q: &Query, out: &mut HashSet<String>) -> Result<()> {
    // 收集本层 CTE 名（排除在物理表之外），并递归收集 CTE 体内的表
    let mut cte_names: HashSet<String> = HashSet::new();
    if let Some(with) = &q.with {
        for cte in &with.cte_tables {
            cte_names.insert(cte.alias.name.value.to_lowercase());
            collect_tables_from_query(&cte.query, out)?;
        }
    }

    collect_tables_from_setexpr(&q.body, out)?;

    // 排除与 CTE 同名的表（大小写不敏感）
    if !cte_names.is_empty() {
        out.retain(|t| !cte_names.contains(&t.to_lowercase()));
    }

    Ok(())
}

fn collect_tables_from_setexpr(se: &SetExpr, out: &mut HashSet<String>) -> Result<()> {
    match se {
        SetExpr::Select(sel) => {
            for twj in &sel.from {
                collect_tables_from_table_with_joins(twj, out)?;
            }
            Ok(())
        }
        SetExpr::Query(q) => collect_tables_from_query(q, out),
        SetExpr::SetOperation { left, right, .. } => {
            collect_tables_from_setexpr(left, out)?;
            collect_tables_from_setexpr(right, out)
        }
        _ => Ok(()),
    }
}

fn collect_tables_from_table_with_joins(twj: &TableWithJoins, out: &mut HashSet<String>) -> Result<()> {
    collect_tables_from_table_factor(&twj.relation, out)?;

    for j in &twj.joins {
        collect_tables_from_join(j, out)?;
    }

    Ok(())
}

fn collect_tables_from_join(j: &Join, out: &mut HashSet<String>) -> Result<()> {
    match &j.join_operator {
        JoinOperator::Inner(_) | JoinOperator::LeftOuter(_) | JoinOperator::RightOuter(_) | JoinOperator::FullOuter(_) => {
            collect_tables_from_table_factor(&j.relation, out)?;
        }
        _ => {}
    }
    Ok(())
}

fn collect_tables_from_table_factor(tf: &TableFactor, out: &mut HashSet<String>) -> Result<()> {
    match tf {
        TableFactor::Table { name, .. } => {
            out.insert(name.to_string());
        }
        TableFactor::Derived { subquery, .. } => {
            collect_tables_from_query(subquery, out)?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_sql_ok_fields() {
        let result = validate_sql("SELECT a FROM t1; SELECT b FROM t2", None);
        assert!(result.ok);
        assert_eq!(result.statement_count, 2);
        assert_eq!(result.statements.len(), 2);
        assert_eq!(result.ast_types, vec!["Query".to_string(), "Query".to_string()]);
        assert_eq!(
            result.normalized_sql.as_deref(),
            Some("SELECT a FROM t1; SELECT b FROM t2")
        );
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_sql_error_fields() {
        let result = validate_sql("SELECTT * FORM t", None);
        assert!(!result.ok);
        assert!(!result.errors.is_empty());
        assert!(result.statements.is_empty());
        assert!(result.ast_types.is_empty());
        assert!(result.normalized_sql.is_none());
    }

    #[test]
    fn test_extract_tables_basic() {
        let tables = extract_tables("SELECT * FROM users u JOIN orders o ON u.id = o.user_id", None).unwrap();
        assert!(tables.contains("users"));
        assert!(tables.contains("orders"));
    }

    #[test]
    fn test_extract_tables_delete() {
        // DELETE FROM：agentlite 正则也匹配 FROM
        let tables = extract_tables("DELETE FROM users WHERE id = 1", None).unwrap();
        assert!(tables.contains("users"));

        // 多表 DELETE + JOIN（MySQL 语法）
        let tables = extract_tables("DELETE t1 FROM t1 JOIN t2 ON t1.id = t2.id", Some("mysql")).unwrap();
        assert!(tables.contains("t1"));
        assert!(tables.contains("t2"));

        // DELETE ... USING（Postgres 语法）
        let tables = extract_tables("DELETE FROM t1 USING t2 WHERE t1.id = t2.id", None).unwrap();
        assert!(tables.contains("t1"));
        assert!(tables.contains("t2"));
    }

    #[test]
    fn test_extract_tables_excludes_cte() {
        let sql = "WITH recent AS (SELECT * FROM raw_events WHERE ts > now()) SELECT * FROM recent";
        let tables = extract_tables(sql, None).unwrap();
        assert!(tables.contains("raw_events"));
        assert!(!tables.contains("recent"), "CTE 名不应作为物理表返回");
    }

    #[test]
    fn test_extract_tables_subquery_and_setop() {
        let tables = extract_tables(
            "SELECT * FROM (SELECT id FROM inner_t) sub UNION SELECT id FROM other_t",
            None,
        )
        .unwrap();
        assert!(tables.contains("inner_t"));
        assert!(tables.contains("other_t"));
    }
}
