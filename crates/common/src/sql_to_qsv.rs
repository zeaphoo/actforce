//! SQL 转 QSV 工具 (简化版)

use anyhow::Result;
use sqlparser::dialect::{GenericDialect, PostgreSqlDialect, MySqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;

/// SQL 方言类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SqlDialect {
    Generic,
    PostgreSQL,
    MySQL,
    SQLite,
}

/// SQL 转 QSV 转换器
pub struct SqlToQsvConverter;

impl SqlToQsvConverter {
    pub fn new() -> Self {
        Self
    }

    /// 解析 SQL 语句
    pub fn parse_sql(&self, sql: &str, dialect: SqlDialect) -> Result<()> {
        let dialect: Box<dyn sqlparser::dialect::Dialect> = match dialect {
            SqlDialect::Generic => Box::new(GenericDialect {}),
            SqlDialect::PostgreSQL => Box::new(PostgreSqlDialect {}),
            SqlDialect::MySQL => Box::new(MySqlDialect {}),
            SqlDialect::SQLite => Box::new(SQLiteDialect {}),
        };

        let _statements = Parser::parse_sql(&*dialect, sql)
            .map_err(|e| anyhow::anyhow!("SQL parse error: {}", e))?;

        Ok(())
    }

    /// 转换 SQL 为 qsv sqlp 兼容的格式
    pub fn to_qsv_sql(&self, sql: &str) -> Result<String> {
        let mut converted = sql.to_string();

        // 1. 移除 MySQL 的反引号
        converted = converted.replace('`', "");

        // 2. 转换 LIMIT offset, count 为 LIMIT count OFFSET offset
        let re = regex::Regex::new(r"LIMIT\s+(\d+)\s*,\s*(\d+)")?;
        converted = re.replace_all(&converted, "LIMIT $2 OFFSET $1").to_string();

        // 3. 转换日期函数
        converted = converted.replace("NOW()", "CURRENT_TIMESTAMP");

        Ok(converted)
    }

    /// 提取 SQL 中的表名
    pub fn extract_tables(&self, sql: &str) -> Vec<String> {
        let mut tables = Vec::new();
        let re = regex::Regex::new(r"(?i)FROM\s+(\w+)|JOIN\s+(\w+)").unwrap();
        
        for cap in re.captures_iter(sql) {
            if let Some(table) = cap.get(1).or_else(|| cap.get(2)) {
                let table_name = table.as_str().to_string();
                if !tables.contains(&table_name) {
                    tables.push(table_name);
                }
            }
        }

        tables
    }

    /// 生成 qsv 命令
    pub fn generate_qsv_command(&self, sql: &str, input_file: &str) -> String {
        format!(
            "qsv sqlp '{}' {}",
            input_file,
            shell_words::quote(sql)
        )
    }
}

/// SQL 格式化工具
pub struct SqlFormatter;

impl SqlFormatter {
    /// 简单的 SQL 格式化
    pub fn format(sql: &str) -> String {
        let mut formatted = String::new();
        let mut indent = 0;

        for token in sql.split_whitespace() {
            let upper = token.to_uppercase();

            if matches!(upper.as_str(), "FROM" | "WHERE" | "GROUP" | "ORDER" | "LIMIT" | "HAVING") {
                if indent > 0 {
                    indent -= 1;
                }
                formatted.push('\n');
            }

            if !formatted.is_empty() && !formatted.ends_with('\n') {
                formatted.push(' ');
            } else if formatted.ends_with('\n') {
                for _ in 0..indent {
                    formatted.push_str("    ");
                }
            }

            formatted.push_str(token);

            if matches!(upper.as_str(), "SELECT" | "FROM" | "WHERE" | "GROUP" | "ORDER") {
                indent += 1;
            }
        }

        formatted
    }

    /// 压缩 SQL（移除多余空白）
    pub fn minify(sql: &str) -> String {
        sql.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

/// SQL 注入检测（基础版）
pub fn check_sql_injection(sql: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    let sql_lower = sql.to_lowercase();

    let dangerous_patterns = [
        (";", "Multiple statements detected"),
        ("--", "Comment detected"),
        ("/*", "Block comment detected"),
        ("drop ", "DROP statement detected"),
        ("delete ", "DELETE statement detected"),
    ];

    for (pattern, warning) in &dangerous_patterns {
        if sql_lower.contains(pattern) {
            warnings.push(warning.to_string());
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sql() {
        let converter = SqlToQsvConverter::new();
        let sql = "SELECT * FROM users WHERE id = 1";
        assert!(converter.parse_sql(sql, SqlDialect::Generic).is_ok());
    }

    #[test]
    fn test_convert_limit() {
        let converter = SqlToQsvConverter::new();
        let sql = "SELECT * FROM users LIMIT 10, 20";
        let result = converter.to_qsv_sql(sql).unwrap();
        assert!(result.contains("LIMIT 20 OFFSET 10"));
    }

    #[test]
    fn test_extract_tables() {
        let converter = SqlToQsvConverter::new();
        let sql = "SELECT * FROM users JOIN orders ON users.id = orders.user_id";
        let tables = converter.extract_tables(sql);
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"orders".to_string()));
    }

    #[test]
    fn test_sql_formatter() {
        let sql = "SELECT * FROM users WHERE id = 1";
        let formatted = SqlFormatter::format(sql);
        assert!(formatted.contains('\n'));
    }

    #[test]
    fn test_check_sql_injection() {
        let sql = "SELECT * FROM users; DROP TABLE users";
        let warnings = check_sql_injection(sql);
        assert!(!warnings.is_empty());
    }
}
