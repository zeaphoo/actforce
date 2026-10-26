# AuPage 标准设计文档

## 1. 概述

**AuPage**（Agent Unified Page）是一种声明式 JSON 文档描述标准，用于统一 agent 系统中分析报告的结构化描述、前端渲染和文档导出。

- **文件后缀**：`.aupage.json`
- **设计目标**：
  1. **数据与展示分离**：分析引擎（如 `agent-analysis`）输出 aupage JSON，前端负责渲染，导出模块负责生成 DOCX/PDF/OFD
  2. **声明式**：描述"展示什么"，而非"怎么渲染"，前端/导出端各自决定渲染方式
  3. **类型化元素**：文档由多种类型的 element 组成，每类 element 有明确的 schema
  4. **前后端一致**：同一份 aupage JSON，前端交互渲染和导出文档使用同一数据源，保证内容一致

## 2. 整体架构

```
┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│  agent-analysis  │────▶│   aupage JSON    │────▶│      前端        │
│  (Rust)          │     │  (.aupage.json)  │     │  (ECharts 渲染)  │
└──────────────────┘     └──────────────────┘     └──────────────────┘
                                │                         │
                                │                         ▼
                                │                   用户交互查看
                                │
                                ▼
                     ┌──────────────────┐
                     │  导出模块         │
                     │  DOCX / PDF / OFD │
                     └──────────────────┘
```

## 3. 文档结构

一个 aupage 文档是一个 JSON 对象，结构如下：

```json
{
  "version": "1.0",
  "title": "文档标题",
  "meta": {
    "page_width": 794,
    "margin": { "top": 72, "right": 72, "bottom": 72, "left": 72 },
    "font_family": "SimSun",
    "font_size": 12
  },
  "elements": [ ... ]
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `version` | string | 是 | aupage 标准版本，当前为 `"1.0"` |
| `title` | string | 是 | 文档标题 |
| `meta` | object | 否 | 文档元信息（生成时间、数据源、页面布局等） |
| `elements` | array | 是 | 文档顶级内容元素（通常为 section） |

### 3.1 Meta 元信息

```json
{
  "meta": {
    "generated_at": "2026-05-19T10:30:00Z",
    "data_source": "lfc.jsonl",
    "page_width": 794,
    "margin": { "top": 72, "right": 72, "bottom": 72, "left": 72 },
    "font_family": "SimSun",
    "font_size": 12
  }
}
```

**Meta 布局字段说明：**

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `page_width` | number | `794` | 页面宽度（px，A4 纸 96 DPI 下为 794px） |
| `margin` | object | `{top:72, right:72, bottom:72, left:72}` | 页面边距（px） |
| `font_family` | string | `"SimSun"` | 默认字体 |
| `font_size` | number | `12` | 默认字号（pt） |

## 4. Element 类型定义

所有 element 共享基础字段：

```json
{
  "type": "<element_type>",
  "id": "optional_unique_id",
  "title": "可选标题",
  "collapsed": false,
  "flex": 1,
  "grow": 1,
  "align": "stretch",
  "justify": "flex-start",
  "gap": 8
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `type` | string | 是 | Element 类型标识 |
| `id` | string | 否 | 唯一标识，用于锚点引用 |
| `title` | string | 否 | Element 标题 |
| `collapsed` | boolean | 否 | 默认是否折叠（仅部分类型有效） |
| `flex` | number / string | 否 | flex 基础值（number 或 `"auto"` / `"none"` / `"content"`） |
| `grow` | number | 否 | flex-grow，默认 `1` |
| `align` | string | 否 | align-self：`"flex-start"` / `"flex-end"` / `"center"` / `"stretch"` |
| `justify` | string | 否 | justify-content（仅 row/column 有效）：`"flex-start"` / `"flex-end"` / `"center"` / `"space-between"` / `"space-around"` |
| `gap` | number | 否 | 子元素间距（px），默认 `8` |

### 4.1 支持的所有 Element 类型

| 类型 | 说明 | 适用场景 |
|------|------|----------|
| `section` | 章节容器 | 顶级页面容器，一个 section ≈ 文档一页 |
| `row` | 行容器 | 水平排列多个元素，flex 布局 |
| `column` | 列容器 | 垂直排列多个元素，flex 布局，可与 row 嵌套 |
| `heading` | 标题 | 章节分隔 |
| `paragraph` | 段落文本 | 文字描述 |
| `markdown` | Markdown 内容 | 富文本（含表格、列表等） |
| `summary_cards` | 摘要卡片 | 关键指标概览 |
| `chart` | 图表 | 数据可视化 |
| `table` | 表格 | 结构化数据展示 |
| `list` | 列表 | 条目列举 |
| `alert` | 警示/提示 | 风险告警、注意事项 |
| `divider` | 分隔线 | 视觉分隔 |
| `image` | 图片 | 外部图片引用 |

---

### 4.2 `section` — 章节容器

`section` 是顶级容器，一个 section 相当于文档的一页。包含 `elements` 属性，可放置任意类型的子元素。

```json
{
  "type": "section",
  "id": "page1",
  "title": "交易流水分析报告",
  "elements": [
    { "type": "heading", "level": 1, "content": "一、交易概览" },
    { "type": "summary_cards", "cards": [ ... ] }
  ]
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `elements` | array | 是 | 子元素数组 |
| `page_break` | string | 否 | 分页控制：`"auto"`（默认，按需分页）/ `"always"`（强制新页开始）/ `"avoid"`（避免分页） |

---

### 4.3 `row` — 行容器

`row` 是水平排列的 flex 容器，`flex-direction: row`。包含 `elements` 属性，可放置任意子元素（包括 `column` 实现多列布局）。

```json
{
  "type": "row",
  "justify": "space-between",
  "gap": 16,
  "elements": [
    {
      "type": "column",
      "flex": 1,
      "elements": [
        { "type": "chart", "chart_type": "bar", "data": { ... } }
      ]
    },
    {
      "type": "column",
      "flex": 1,
      "elements": [
        { "type": "chart", "chart_type": "pie", "data": { ... } }
      ]
    }
  ]
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `elements` | array | 是 | 子元素数组 |
| `justify` | string | 否 | justify-content，默认 `"flex-start"` |
| `gap` | number | 否 | 子元素间距（px），默认 `8` |
| `wrap` | boolean | 否 | flex-wrap，默认 `false` |

---

### 4.4 `column` — 列容器

`column` 是垂直排列的 flex 容器，`flex-direction: column`。包含 `elements` 属性，可放置任意子元素（包括 `row` 实现嵌套行布局）。

```json
{
  "type": "column",
  "flex": 1,
  "gap": 12,
  "elements": [
    { "type": "heading", "level": 2, "content": "收入分析" },
    { "type": "chart", "chart_type": "line", "data": { ... } },
    { "type": "table", "columns": [ ... ], "rows": [ ... ] }
  ]
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `elements` | array | 是 | 子元素数组 |
| `align` | string | 否 | align-items，默认 `"stretch"` |
| `gap` | number | 否 | 子元素间距（px），默认 `8` |

---

### 4.5 `heading` — 标题

```json
{
  "type": "heading",
  "level": 1,
  "content": "一、交易概览"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `level` | number | 是 | 标题级别，1-4 |
| `content` | string | 是 | 标题文字 |

---

### 4.6 `paragraph` — 段落文本

```json
{
  "type": "paragraph",
  "content": "该账户在统计期间共发生交易 686 笔，总收入 ¥685,432.10，总支出 ¥523,891.00。"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `content` | string | 是 | 段落文字 |

---

### 4.7 `markdown` — Markdown 内容

```json
{
  "type": "markdown",
  "content": "| 风险类型 | 是否检出 | 置信度 |\n|---------|---------|--------|\n| 账户突然活跃 | 是 | 85% |"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `content` | string | 是 | Markdown 文本 |

---

### 4.8 `summary_cards` — 摘要卡片

```json
{
  "type": "summary_cards",
  "cards": [
    {
      "label": "总收入",
      "value": "¥685,432.10",
      "trend": "+12.5%",
      "trend_type": "up"
    },
    {
      "label": "总支出",
      "value": "¥523,891.00",
      "trend": "-5.3%",
      "trend_type": "down"
    },
    {
      "label": "净收入",
      "value": "¥161,541.10"
    },
    {
      "label": "交易笔数",
      "value": "686"
    }
  ]
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cards` | array | 是 | 卡片数组 |
| `cards[].label` | string | 是 | 卡片标签 |
| `cards[].value` | string | 是 | 卡片数值 |
| `cards[].trend` | string | 否 | 趋势值（如 "+12.5%"） |
| `cards[].trend_type` | string | 否 | 趋势方向：`"up"` / `"down"` / `"neutral"` |
| `cards[].color` | string | 否 | 卡片强调色 |

---

### 4.9 `chart` — 图表

```json
{
  "type": "chart",
  "chart_type": "bar",
  "title": "收入区间分布",
  "data": {
    "categories": ["0-11万", "11-22万", "22-33万", "33-44万", "44-55万", "55-67万"],
    "series": [
      {
        "name": "笔数",
        "values": [120, 85, 42, 18, 5, 2]
      }
    ]
  },
  "config": {
    "x_axis_name": "金额区间",
    "y_axis_name": "笔数",
    "stack": false,
    "show_label": true
  }
}
```

**支持的 `chart_type`：**

| 值 | 说明 | 适用场景 |
|----|------|----------|
| `bar` | 柱状图 | 分类对比 |
| `column` | 条形图 | 分类对比（水平） |
| `line` | 折线图 | 趋势变化 |
| `area` | 面积图 | 趋势 + 量级 |
| `pie` | 饼图 | 占比分布 |
| `ring` | 环形图 | 占比分布 |
| `scatter` | 散点图 | 分布关系 |
| `radar` | 雷达图 | 多维对比 |

**`data` 结构：**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `categories` | string[] | 是 | X 轴分类标签（饼图/环形图为各项名称） |
| `series` | array | 是 | 数据系列 |
| `series[].name` | string | 是 | 系列名称 |
| `series[].values` | number[] | 是 | 系列数值（长度须与 categories 一致） |

**`config` 可选配置：**

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `x_axis_name` | string | `""` | X 轴名称 |
| `y_axis_name` | string | `""` | Y 轴名称 |
| `stack` | boolean | `false` | 是否堆叠（bar/line/area 有效） |
| `show_label` | boolean | `false` | 是否显示数据标签 |
| `show_legend` | boolean | `true` | 是否显示图例 |
| `unit` | string | `""` | 数值单位（如 "元"、"笔"） |

---

### 4.10 `table` — 表格

```json
{
  "type": "table",
  "title": "交易对手分析 TOP 5",
  "columns": [
    { "key": "name", "label": "交易对手", "width": "30%" },
    { "key": "total_in", "label": "总收入(元)", "align": "right" },
    { "key": "total_out", "label": "总支出(元)", "align": "right" },
    { "key": "count", "label": "笔数", "align": "center" }
  ],
  "rows": [
    { "name": "李法昌", "total_in": "50000.00", "total_out": "0.00", "count": "12" },
    { "name": "支付宝", "total_in": "0.00", "total_out": "35000.00", "count": "89" }
  ]
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `columns` | array | 是 | 列定义 |
| `columns[].key` | string | 是 | 列数据键 |
| `columns[].label` | string | 是 | 列标题 |
| `columns[].width` | string | 否 | 列宽（百分比或 px） |
| `columns[].align` | string | 否 | 对齐：`"left"` / `"right"` / `"center"` |
| `rows` | array | 是 | 行数据 |
| `rows[].{key}` | string | 是 | 单元格值（统一为 string，由前端决定格式化） |

---

### 4.11 `list` — 列表

```json
{
  "type": "list",
  "list_type": "ordered",
  "items": [
    "2026-01-05 22:48 跨行转账收入 ¥6,000.00",
    "2026-01-09 10:14 网上快捷支出 ¥9.79"
  ]
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `list_type` | string | 否 | `"ordered"`（有序）或 `"unordered"`（无序），默认 `"unordered"` |
| `items` | string[] | 是 | 列表项 |

---

### 4.12 `alert` — 警示/提示

```json
{
  "type": "alert",
  "alert_type": "warning",
  "title": "风险检测：账户突然活跃",
  "content": "涉案前7天内发生12笔交易，交易笔数占比61.5%，交易金额占比78.3%。",
  "confidence": 0.85
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `alert_type` | string | 是 | `"error"` / `"warning"` / `"info"` / `"success"` |
| `title` | string | 否 | 警示标题 |
| `content` | string | 是 | 警示内容 |
| `confidence` | number | 否 | 置信度 (0-1) |

---

### 4.13 `divider` — 分隔线

```json
{
  "type": "divider"
}
```

无额外字段。

---

### 4.14 `image` — 图片

```json
{
  "type": "image",
  "src": "data:image/png;base64,iVBORw0KGgo...",
  "alt": "交易趋势图",
  "width": 600,
  "caption": "图1：2026年1-6月收支趋势"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `src` | string | 是 | 图片 URL 或 base64 data URI |
| `alt` | string | 否 | 替代文字 |
| `width` | number | 否 | 显示宽度（px） |
| `caption` | string | 否 | 图片说明 |

## 5. 完整示例

以下是一个交易流水分析报告的 aupage 示例，使用 section/row/column 进行排版：

```json
{
  "version": "1.0",
  "title": "交易流水分析报告",
  "meta": {
    "generated_at": "2026-05-19T10:30:00Z",
    "data_source": "lfc.jsonl",
    "page_width": 794,
    "margin": { "top": 72, "right": 72, "bottom": 72, "left": 72 }
  },
  "elements": [
    {
      "type": "section",
      "id": "overview",
      "title": "交易概览",
      "elements": [
        { "type": "heading", "level": 1, "content": "一、交易概览" },
        {
          "type": "summary_cards",
          "cards": [
            { "label": "总收入", "value": "¥685,432.10", "trend": "+12.5%", "trend_type": "up" },
            { "label": "总支出", "value": "¥523,891.00", "trend": "-5.3%", "trend_type": "down" },
            { "label": "净收入", "value": "¥161,541.10" },
            { "label": "交易笔数", "value": "686" }
          ]
        }
      ]
    },
    {
      "type": "section",
      "id": "trends",
      "title": "收支趋势",
      "elements": [
        { "type": "heading", "level": 1, "content": "二、月度收支趋势" },
        {
          "type": "chart",
          "chart_type": "line",
          "title": "月度收入与支出趋势",
          "data": {
            "categories": ["1月", "2月", "3月", "4月", "5月", "6月"],
            "series": [
              { "name": "收入", "values": [120000, 95000, 150000, 110000, 105432, 105000] },
              { "name": "支出", "values": [80000, 70000, 120000, 90000, 83891, 80000] }
            ]
          },
          "config": {
            "x_axis_name": "月份",
            "y_axis_name": "金额(元)",
            "show_label": true
          }
        }
      ]
    },
    {
      "type": "section",
      "id": "distribution",
      "title": "分布分析",
      "elements": [
        { "type": "heading", "level": 1, "content": "三、分布分析" },
        {
          "type": "row",
          "gap": 16,
          "elements": [
            {
              "type": "column",
              "flex": 1,
              "elements": [
                { "type": "heading", "level": 2, "content": "收入区间分布" },
                {
                  "type": "chart",
                  "chart_type": "bar",
                  "data": {
                    "categories": ["0-11万", "11-22万", "22-33万", "33-44万", "44-55万", "55-67万"],
                    "series": [{ "name": "笔数", "values": [120, 85, 42, 18, 5, 2] }]
                  },
                  "config": { "x_axis_name": "金额区间", "y_axis_name": "笔数" }
                }
              ]
            },
            {
              "type": "column",
              "flex": 1,
              "elements": [
                { "type": "heading", "level": 2, "content": "收入来源占比" },
                {
                  "type": "chart",
                  "chart_type": "pie",
                  "data": {
                    "categories": ["李法昌", "工资收入", "其他"],
                    "series": [{ "name": "金额", "values": [450000, 185432, 50000] }]
                  }
                }
              ]
            }
          ]
        }
      ]
    },
    {
      "type": "section",
      "id": "counterparty",
      "title": "交易对手",
      "elements": [
        { "type": "heading", "level": 1, "content": "四、交易对手分析" },
        {
          "type": "table",
          "title": "交易对手 TOP 5",
          "columns": [
            { "key": "name", "label": "交易对手" },
            { "key": "total_in", "label": "总收入(元)", "align": "right" },
            { "key": "total_out", "label": "总支出(元)", "align": "right" },
            { "key": "count", "label": "笔数", "align": "center" }
          ],
          "rows": [
            { "name": "李法昌", "total_in": "450000.00", "total_out": "0.00", "count": "52" },
            { "name": "工资收入", "total_in": "185432.10", "total_out": "0.00", "count": "6" },
            { "name": "支付宝", "total_in": "0.00", "total_out": "120000.00", "count": "230" }
          ]
        }
      ]
    },
    {
      "type": "section",
      "id": "risk",
      "title": "风险检测",
      "elements": [
        { "type": "heading", "level": 1, "content": "五、风险检测" },
        {
          "type": "alert",
          "alert_type": "warning",
          "title": "账户突然活跃",
          "content": "涉案前7天内发生12笔交易，交易笔数占比61.5%，交易金额占比78.3%。",
          "confidence": 0.85
        },
        {
          "type": "alert",
          "alert_type": "warning",
          "title": "资金归集后分散转出",
          "content": "收入集中于2个账户，支出分散到8个不同账户，前2个收入账户占比72.1%。",
          "confidence": 0.72
        },
        {
          "type": "alert",
          "alert_type": "info",
          "title": "节假日交易",
          "content": "节假日期间交易占比33.4%（共229笔，金额811,014.73元），涉及节日：周末(168笔), 春节(26笔), 国庆(12笔)。"
        }
      ]
    },
    {
      "type": "section",
      "id": "fake_detection",
      "title": "虚假流水分析",
      "elements": [
        { "type": "heading", "level": 1, "content": "六、虚假流水分析" },
        {
          "type": "alert",
          "alert_type": "error",
          "title": "余额连贯性异常",
          "content": "共校验320条相邻记录，发现15条余额不连贯。",
          "confidence": 0.75
        },
        {
          "type": "paragraph",
          "content": "综上分析，该账户交易存在多处异常特征，建议进一步核查。"
        }
      ]
    }
  ]
}
```

## 6. 与现有系统的映射

### 6.1 agent-analysis 输出 → aupage

`agent-analysis` 的 `analyze_transactions_full()` 返回的 JSON 可以转换为 aupage：

| agent-analysis 字段 | aupage element |
|---------------------|----------------|
| `summary_net` | `summary_cards` |
| `monthly_trends` | `chart` (line) |
| `income_range_analysis` | `chart` (bar) |
| `expense_range_analysis` | `chart` (bar) |
| `top_10_income` / `top_10_expense` | `table` |
| `proportions.by_type` | `chart` (pie) |
| `risk_detection.features` | `alert` |
| `fake_transaction_analysis.features` | `alert` |
| `time_interval_stats` | `chart` (bar) |
| `usage_stats` | `table` |

### 6.2 前端渲染

- `chart` → ECharts 组件
- `table` → 表格组件
- `summary_cards` → 卡片组件
- `alert` → 警示组件
- `markdown` → Markdown 渲染
- `heading` / `paragraph` / `list` / `divider` / `image` → 对应基础组件

### 6.3 导出 DOCX/PDF

导出时遍历 elements：
- `chart` → 用 ECharts SSR 或 Plotters 生成 PNG → 嵌入文档
- `table` → 文档原生表格
- `summary_cards` → 表格或段落
- `alert` → 带样式的段落
- `heading` / `paragraph` → 文档原生段落
- `markdown` → 解析后转为文档元素

## 7. 版本管理

- 版本号采用 `"major.minor"` 格式
- 当前版本：`"1.0"`
- 向后兼容原则：新增 element 类型或可选字段不影响已有解析器
