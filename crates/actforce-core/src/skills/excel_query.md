---
id: sk_excel_query
name: Excel数据查询分析
description: 接受用户上传的Excel/CSV文件，根据用户提出的问题，智能生成SQL查询或qsv数据处理命令，执行后返回查询结果。支持多文件关联查询、统计分析、数据筛选等操作。
is_enabled: true
program: null
extra_json:
  icon: Table2
  category: 数据分析
  tags:
  - Excel
  - CSV
  - SQL查询
  - 数据分析
  is_builtin: true
---

# Excel数据查询分析 Skill

## 适用场景

用户上传一个或多个 Excel（.xlsx/.xls）或 CSV 文件，并提出关于数据的自然语言问题。
本 Skill 自动解析文件结构，生成 SQL 查询或 qsv 命令，执行后返回分析结果。

**典型用户问题示例**：
- "这个表里销售额最高的前10个产品是什么？"
- "帮我统计每个月的订单总金额"
- "两个表中相同的客户有哪些？"
- "数据中有没有重复记录？"
- "导出所有金额大于1万的交易到新的Excel文件"

---

## 执行流程

本 Skill 通过 3 个步骤完成 Excel/CSV 数据的查询分析任务。

---

### 步骤1：文件探测与Schema提取

**类型**: `file_process`

**执行动作**: 检测用户上传的文件类型，将Excel转为CSV，提取每个文件的Schema信息。

**处理流程**：
1. 判断文件类型（.xlsx/.xls → Excel，.csv → CSV）
2. Excel文件：生成 `qsv excel <file> -o <output.csv>` 转换命令
3. CSV文件：执行以下探测命令
   - `qsv headers <file.csv>` — 获取列名
   - `qsv count <file.csv>` — 获取行数
   - `qsv stats <file.csv> --everything` — 获取详细统计
   - `qsv sniff <file.csv>` — 快速嗅探元数据

**输出**: 每个文件的Schema描述（列名、类型、行数、统计摘要）

**output_target**:
  - **output_type**: "context"
  - **name**: "文件Schema信息"
  - **tags**: ["file_schema", "csv_info"]

**validation**: 检查是否成功提取至少一个文件的Schema（列名非空、行数>0）

**validation_failure_handling**:
- **可忽略**: false
- **失败提示**: "文件解析失败，无法提取数据结构信息。请确认上传的文件为有效的Excel或CSV格式。"

---

### 步骤2：生成并执行查询命令

**类型**: `generate_command`

**执行动作**: 根据用户问题和Schema信息，生成qsv命令并执行。

**可用命令说明**：

#### SQL查询（核心能力）
```
qsv sqlp <file1.csv> [file2.csv ...] '<SQL>'
```
- SQL方言：PostgreSQL兼容的Polars SQL
- 多文件关联：文件名即为表名
- 列名含空格时用双引号：`"列 名"`
- 支持：JOIN、GROUP BY、ORDER BY、HAVING、子查询、UNION、聚合函数

**示例**：
```
# 单文件聚合查询
qsv sqlp sales.csv 'SELECT product, SUM(amount) as total FROM sales GROUP BY product ORDER BY total DESC LIMIT 10'

# 多文件关联查询
qsv sqlp orders.csv customers.csv 'SELECT c.name, COUNT(o.id) as order_count FROM orders o JOIN customers c ON o.customer_id = c.id GROUP BY c.name'

# 输出为Excel文件
qsv sqlp data.csv 'SELECT * FROM data WHERE amount > 10000' --format xlsx --output result.xlsx
```

#### 其他qsv命令
- `qsv stats <file.csv> --everything` — 详细统计
- `qsv frequency <file.csv> --select <column>` — 频率分布
- `qsv search <regex> <file.csv>` — 正则搜索
- `qsv sort <file.csv> -s <column>` — 排序
- `qsv dedup <file.csv>` — 去重
- `qsv sample <count> <file.csv>` — 随机抽样

**生成规则**：
1. 优先使用 `qsv sqlp`，功能最强大
2. 多文件问题使用多文件SQL
3. 大数据量默认加 `LIMIT 100`
4. 需要导出文件时添加 `--output` 和 `--format`

**output_target**:
  - **output_type**: "context"
  - **name**: "查询执行结果"
  - **tags**: ["query_result"]
**context_query**: ["file_schema", "csv_info"]

**validation**: 检查命令执行成功（exit code 0）且输出包含有效数据

**validation_failure_handling**:
- **可忽略**: false
- **失败提示**: "查询执行失败，请检查SQL语法或列名是否正确。"

---

### 步骤3：结果解读与回答生成

**类型**: `answer`

**执行动作**: 基于查询结果生成清晰的数据分析回答。

**回答规范**：
1. 先用一句话直接回答核心问题
2. 数值数据用表格或列表展示
3. 多行数据提供关键摘要（总计、平均、最大/最小）
4. 如有输出文件，告知用户路径和下载方式
5. 结果为空时解释原因并建议调整

**output_target**:
  - **output_type**: "context"
  - **name**: "最终回答"
  - **tags**: ["final_answer"]
**context_query**: ["query_result"]

**validation**: 验证输出非空且与用户问题相关

**validation_failure_handling**:
- **可忽略**: false
- **失败提示**: "回答生成失败，请重试或换一种方式提问。"

---
