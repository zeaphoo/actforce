---
id: sk_document_organize
name: 文档整理
description: 将图片和文档中的信息智能识别，整理为结构化文档，用户提供了一组散乱的资料，希望整理并得到一个文档。
is_enabled: true
program: null
extra_json:
  icon: FileStack
  category: 文档处理
  tags:
    - 文档整理
    - 智能识别
  is_builtin: true
---

# 文档整理 Skill

## 执行流程

### 步骤1：文档分类与内容提取

**步骤类型**: `file_process`

**参数**:
- **file_paths**: `$attachments`
- **skip_if_none**: true

**执行动作**: 分析文件内容或元数据，判断属于图片/证件、表格还是文本类

**LLM 调用**:
- **System Prompt**: 你是一个专业的文档解析助手。请严格根据输入的文件信息，识别其类型并输出对应的JSON格式。禁止输出任何额外文本或Markdown标记。
- **User Prompt**: 请按照上述规范输出Markdown格式

**output_target**:
  - **output_type**: "file"
  - **name**: "单文档分类与内容提取"
  - **tags**: ["document_classification"]
**validation**: 检查有内容输出
**validation_failure_handling**: 若验证失败，自动追加格式强约束提示重新调用LLM重试一次；若仍失败则标记为无效并记录错误日志

---

### 步骤2：逐个文件提取概要

**步骤类型**: `loop_block`

**参数**:
- **items**: `@llm:读取所有的文件路径信息, 返回JSON数组格式，每个元素包含文件路径和文件名`
- **skip_if_none**: true

**执行动作**: 逐个文件提取概要

**子步骤**:

#### 2.1 读取文件内容
- **步骤类型**: `generate_command`
- **执行动作**: 读取文件内容
- **LLM 调用**:
  - **System Prompt**: 你是Bash脚本专家，写一组脚本将文件内容读取并输出。
  - **User Prompt**: 请读取文件内容并输出。
- **output_target**:
  - **output_type**: "context"
  - **name**: "内容提取结果汇总与回复"
  - **tags**: ["内容提取结果汇总与回复"]

#### 2.2 提炼摘要信息和可提炼出的信息类型
- **步骤类型**: `call_llm`
- **执行动作**: 读取摘要信息提炼出的信息类型
- **LLM 调用**:
  - **System Prompt**: 你是一个专业的文档解析助手。请根据输入的文件内容，提取摘要信息和可提炼出的信息类型。请以JSON格式返回，必须包含"summary"（摘要信息）和"extractable_types"（可提炼出的信息类型）两个字段。
  - **User Prompt**: 请分析以下文件，提取其中的摘要信息和可提炼出的信息类型。请以JSON格式返回，必须包含"summary"（摘要信息）和"extractable_types"（可提炼出的信息类型）两个字段。
- **output_target**:
  - **output_type**: "context"
  - **target_block**: "root"
  - **name**: "逐个文件提取概要"
  - **tags**: ["$summary"]

**output_target**:
  - **output_type**: "context"
  - **name**: "多类型文件内容提取路由"
  - **tags**: ["多类型文件内容提取路由"]

---

### 步骤3：内容提取结果汇总与回复

**步骤类型**: `loop_block`

**参数**:
- **items**: `@llm: 提炼出的信息类型， 将附件分组，每组包括可以提炼的类型和对应的文件路径信息。JSON格式返回，返回一组数据， 每个元素包含"extractable_types"和"file_paths"两个字段。`
- **skip_if_none**: true

**执行动作**: 整合各文件类型的提取结果，生成最终的内容提取报告并返回给用户

**LLM 调用**:
- **System Prompt**: 你是一个文档整理助手。请根据前面步骤提取的结构化内容，生成清晰、准确的最终回复。
- **User Prompt**: 以下是各文件的内容提取结果：\n{context}\n\n请总结提取到的核心信息，并以易读的格式呈现。如果某类文件提取失败，请说明原因。

**子步骤**:

#### 3.1 读取文件内容
- **步骤类型**: `generate_command`
- **执行动作**: 读取文件内容
- **LLM 调用**:
  - **System Prompt**: 你是Bash脚本专家，写一组脚本将文件内容读取并输出。
  - **User Prompt**: 请读取文件内容并输出。
- **output_target**:
  - **output_type**: "context"
  - **name**: "内容提取结果汇总与回复"
  - **tags**: ["内容提取结果汇总与回复"]

#### 3.2 生成最终内容提取报告
- **步骤类型**: `call_llm`
- **执行动作**: 生成最终内容提取报告
- **LLM 调用**:
  - **System Prompt**: 你是一个文档整理助手。请根据前面步骤提取的结构化内容，生成清晰、准确的最终回复。
  - **User Prompt**: 请总结提取到的核心信息，并以易读的格式呈现。如果某类文件提取失败，请说明原因。
- **output_target**:
  - **output_type**: "file"
  - **name**: "内容提取结果汇总与回复"
  - **tags**: ["内容提取结果汇总与回复"]
  - **identify_artifacts**: true

**output_target**:
  - **output_type**: "context"
  - **name**: "内容提取结果汇总与回复"
  - **tags**: ["内容提取结果汇总与回复"]
**validation**: 检查最终回复是否包含所有已处理文件的提取摘要，且无乱码或严重截断
**validation_failure_handling**: 若回复质量不达标，重新调用LLM基于原始中间产物进行二次生成

---

### 步骤4：生成最终整理报告

**步骤类型**: `answer`

**执行动作**: 汇总循环处理的所有文档结果，按统一格式输出最终整理清单

**LLM 调用**:
- **System Prompt**: 你是一个文档整理汇报助手。请根据提供的已处理文档结果，生成一份清晰的结构化整理报告。
- **User Prompt**: 已处理的文档结果：\n请输出最终的文档整理清单：

**context_query**: ["processed_docs"]

**output_target**:
  - **output_type**: "context"
  - **name**: "生成最终整理报告"
  - **tags**: ["final_report"]

---
