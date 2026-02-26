# XClaw 开发计划

> 基于 MicroClaw 思路的 Python 多功能 Agent 运行时，聚焦任务处理 & 股票投资助手

---

## 目录

1. [项目概述](#1-项目概述)
2. [核心设计原则](#2-核心设计原则)
3. [技术栈选型](#3-技术栈选型)
4. [系统架构](#4-系统架构)
5. [核心模块设计](#5-核心模块设计)
6. [投资助手专项功能](#6-投资助手专项功能)
7. [任务处理系统](#7-任务处理系统)
8. [安全设计](#8-安全设计)
9. [分阶段开发路线图](#9-分阶段开发路线图)
10. [目录结构参考](#10-目录结构参考)
11. [与 MicroClaw 功能对照](#11-与-microclaw-功能对照)

---

## 1. 项目概述

### 定位

XClaw 是一个基于 Python 的智能 Agent 运行时系统，继承 MicroClaw 的核心架构理念（渠道无关的 Agent 循环、Provider 无关的 LLM 层、工具注册体系），聚焦两大领域：

- **任务处理**：通用的 agentic 工作流（工具调用、计划执行、调度任务）
- **股票投资助手**：行情获取、技术分析、投资组合跟踪、自动化研报摘要

### 目标用户

个人开发者 / 个人投资者，通过 Telegram 或 Web 界面与 Agent 交互，获取投资辅助决策。

### 核心能力矩阵

| 能力 | 说明 |
|------|------|
| 多步工具调用 | Agent 循环：LLM → 工具 → 结果 → 继续，最多 N 轮 |
| 会话持久化 | 完整对话状态（含工具调用链）存入 SQLite，重启可恢复 |
| 上下文压缩 | 当消息过多时自动摘要旧消息，保留近期上下文 |
| 持久记忆 | 文件记忆（Markdown）+ 结构化记忆（SQLite） |
| 定时任务 | cron 表达式 + 一次性任务，后台轮询执行 |
| 投资数据 | 实时行情、历史 K 线、技术指标、财务数据 |
| 渠道适配 | Telegram + Web（前期），可扩展 Discord 等 |

---

## 2. 核心设计原则

1. **简单优先**：最小可行架构，不过度抽象；一个 `main.py` 能启动全部服务。
2. **实用便利**：即装即用，`pip install` + 配置文件即可运行；投资工具开箱可用。
3. **基础安全**：敏感路径拦截、工具风险分级、API Key 不写入日志、命令执行可控。
4. **渐进扩展**：核心 Agent 循环与渠道/工具解耦，新增工具只需一个 Python 文件。
5. **借鉴但不照搬**：从 MicroClaw 学习架构模式，但充分利用 Python 生态的便利性。

---

## 3. 技术栈选型

| 层次 | MicroClaw (Rust) | XClaw (Python) | 选型理由 |
|------|------------------|----------------|----------|
| 语言 | Rust 2021 | **Python 3.11+** | 生态丰富、金融库完善 |
| 异步运行时 | Tokio | **asyncio** | Python 标准异步方案 |
| LLM 客户端 | reqwest 直接 HTTP | **httpx** (异步) | 原生 async、HTTP/2 支持 |
| LLM 类型系统 | 自定义 DTO | **pydantic v2** | 数据校验 + JSON Schema 生成 |
| Telegram | teloxide | **python-telegram-bot v21+** | 官方维护、异步原生 |
| Web 框架 | axum | **FastAPI** | 自带 OpenAPI、SSE 支持 |
| 数据库 | rusqlite (bundled) | **aiosqlite** + sqlite3 | 异步 SQLite、零部署 |
| 配置 | serde + YAML | **pydantic-settings** + YAML | 类型安全 + 环境变量回退 |
| 股票数据 | — | **akshare** / yfinance | A 股优先用 akshare，美股用 yfinance |
| 技术分析 | — | **pandas-ta** | 纯 Python、指标全面 |
| 数据处理 | — | **pandas** | 金融数据标准处理框架 |
| 定时调度 | cron crate + 轮询 | **APScheduler** | 成熟的 Python 调度库 |
| 日志 | tracing | **loguru** | 简单强大、结构化日志 |
| CLI | clap | **click** / typer | 声明式 CLI、自动 help |
| 前端 | React + Vite | **React + Vite**（复用方案） | 沿用 MicroClaw 方案 |

---

## 4. 系统架构

```
┌──────────────────────────────────────────────────────┐
│                    XClaw Runtime                      │
│                                                      │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐              │
│  │Telegram │  │  Web    │  │ (扩展)  │  ← 渠道适配  │
│  │ Adapter │  │ Adapter │  │ Discord │              │
│  └────┬────┘  └────┬────┘  └────┬────┘              │
│       │            │            │                    │
│       └────────────┼────────────┘                    │
│                    ▼                                 │
│          ┌─────────────────┐                         │
│          │  Agent Engine   │  ← 核心 Agent 循环      │
│          │  (agent_loop)   │                         │
│          └───────┬─────────┘                         │
│                  │                                   │
│       ┌──────────┼──────────┐                        │
│       ▼          ▼          ▼                        │
│  ┌─────────┐ ┌────────┐ ┌──────────┐                │
│  │  LLM    │ │ Tool   │ │ Memory   │                │
│  │Provider │ │Registry│ │ Manager  │                │
│  └─────────┘ └───┬────┘ └──────────┘                │
│                  │                                   │
│    ┌─────────────┼──────────────────┐                │
│    ▼       ▼     ▼     ▼       ▼    ▼                │
│  [bash] [file] [web] [stock] [sched] [memory]        │
│                                                      │
│  ┌──────────┐  ┌──────────┐                          │
│  │ Database │  │Scheduler │  ← 持久化 + 定时         │
│  │ (SQLite) │  │(APSched) │                          │
│  └──────────┘  └──────────┘                          │
└──────────────────────────────────────────────────────┘
```

---

## 5. 核心模块设计

### 5.1 Agent Engine（Agent 循环）

参照 MicroClaw 的 `process_with_agent` 设计：

```python
async def agent_loop(
    context: AgentContext,
    user_message: str,
    max_iterations: int = 50,
) -> str:
    """
    核心 Agent 循环:
    1. 快速记忆路径: 检测 "记住..." 直接写入结构化记忆
    2. 加载会话: 从 sessions 表恢复，或从历史重建
    3. 构建 system prompt: 文件记忆 + 结构化记忆 + 工具目录
    4. 上下文压缩: 超限时摘要旧消息
    5. 调用 LLM (带工具定义)
    6. 工具循环: tool_use → 执行 → 追加结果 → 重新调用
    7. 持久化会话并返回文本
    """
```

**关键设计点**：
- `AgentContext` 包含: `chat_id`, `channel`, `chat_type`, `db`, `llm`, `tools`, `memory`
- 工具调用结果追加到消息列表，保持完整上下文
- 会话 JSON 序列化存入 `sessions` 表，支持重启恢复
- 上下文压缩：当消息数超过 `max_session_messages` 时，用 LLM 摘要旧消息

### 5.2 LLM Provider 抽象

```python
class LLMProvider(Protocol):
    """LLM 提供商抽象接口"""

    async def chat(
        self,
        messages: list[Message],
        tools: list[ToolDefinition] | None = None,
        max_tokens: int = 4096,
    ) -> LLMResponse: ...

    async def chat_stream(
        self,
        messages: list[Message],
        tools: list[ToolDefinition] | None = None,
        max_tokens: int = 4096,
    ) -> AsyncIterator[LLMEvent]: ...
```

**初期实现**：
- `AnthropicProvider`：直接调用 Anthropic Messages API (httpx)
- `OpenAICompatibleProvider`：兼容 OpenAI / DeepSeek / Ollama 等

**数据类型** (Pydantic)：
- `Message`：统一消息格式（role, content blocks）
- `ToolDefinition`：JSON Schema 工具描述
- `ToolUseBlock` / `ToolResultBlock`：工具调用 & 结果
- `LLMResponse`：stop_reason + content blocks + usage stats

### 5.3 Tool 系统

参照 MicroClaw 的 `Tool` trait + `ToolRegistry` 模式：

```python
class Tool(ABC):
    """工具基类"""

    @property
    @abstractmethod
    def name(self) -> str: ...

    @property
    @abstractmethod
    def description(self) -> str: ...

    @property
    @abstractmethod
    def parameters(self) -> dict:
        """返回 JSON Schema 格式的参数定义"""
        ...

    @property
    def risk_level(self) -> RiskLevel:
        return RiskLevel.LOW

    @abstractmethod
    async def execute(self, params: dict, context: ToolContext) -> ToolResult: ...


class ToolRegistry:
    """工具注册表，管理所有可用工具"""

    def register(self, tool: Tool) -> None: ...
    def get_definitions(self) -> list[ToolDefinition]: ...
    async def execute(self, name: str, params: dict, ctx: ToolContext) -> ToolResult: ...
```

**添加新工具**只需：
1. 创建 `xclaw/tools/my_tool.py`，实现 `Tool` 抽象类
2. 在 `ToolRegistry` 初始化时注册

### 5.4 记忆系统

**双层设计**（沿用 MicroClaw 方案）：

| 层 | 存储 | 用途 |
|----|------|------|
| 文件记忆 | `data/groups/{chat_id}/AGENTS.md` | 自由格式的长期笔记，注入 system prompt |
| 结构化记忆 | SQLite `memories` 表 | 分类事实，带置信度、归档、去重 |

**记忆质量规则**（简化版）：
- 显式 "记住..." 指令 → 直接写入
- 置信度阈值 (0.0-1.0)
- Jaccard 相似度去重
- 软归档而非硬删除

### 5.5 数据库

```sql
-- 核心表
CREATE TABLE chats (
    id INTEGER PRIMARY KEY,
    channel TEXT NOT NULL,           -- 'telegram' | 'web'
    external_chat_id TEXT NOT NULL,
    chat_type TEXT DEFAULT 'private',
    title TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    UNIQUE(channel, external_chat_id)
);

CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    chat_id INTEGER REFERENCES chats(id),
    role TEXT NOT NULL,              -- 'user' | 'assistant' | 'system'
    content TEXT NOT NULL,
    sender_name TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE sessions (
    chat_id INTEGER PRIMARY KEY REFERENCES chats(id),
    messages_json TEXT NOT NULL,     -- 完整会话状态 (含 tool_use/tool_result)
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE scheduled_tasks (
    id INTEGER PRIMARY KEY,
    chat_id INTEGER REFERENCES chats(id),
    description TEXT NOT NULL,
    cron_expression TEXT,            -- NULL = 一次性
    prompt TEXT NOT NULL,
    status TEXT DEFAULT 'active',    -- active | paused | cancelled | completed
    next_run_at TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE memories (
    id INTEGER PRIMARY KEY,
    chat_id INTEGER REFERENCES chats(id),
    content TEXT NOT NULL,
    category TEXT,
    confidence REAL DEFAULT 0.8,
    source TEXT DEFAULT 'explicit',  -- explicit | reflector
    is_archived INTEGER DEFAULT 0,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

-- 投资专用表
CREATE TABLE watchlist (
    id INTEGER PRIMARY KEY,
    chat_id INTEGER REFERENCES chats(id),
    symbol TEXT NOT NULL,
    market TEXT DEFAULT 'CN',       -- CN | US | HK
    name TEXT,
    notes TEXT,
    added_at TEXT DEFAULT (datetime('now')),
    UNIQUE(chat_id, symbol, market)
);

CREATE TABLE portfolio (
    id INTEGER PRIMARY KEY,
    chat_id INTEGER REFERENCES chats(id),
    symbol TEXT NOT NULL,
    market TEXT DEFAULT 'CN',
    shares REAL NOT NULL,
    avg_cost REAL NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')),
    UNIQUE(chat_id, symbol, market)
);

CREATE TABLE llm_usage (
    id INTEGER PRIMARY KEY,
    chat_id INTEGER,
    model TEXT,
    input_tokens INTEGER,
    output_tokens INTEGER,
    created_at TEXT DEFAULT (datetime('now'))
);
```

### 5.6 渠道适配

```python
class ChannelAdapter(ABC):
    """渠道适配器抽象"""

    @abstractmethod
    async def start(self) -> None: ...

    @abstractmethod
    async def send_response(self, chat_id: str, text: str) -> None: ...
```

**初期实现**：
- `TelegramAdapter`：基于 python-telegram-bot，处理私聊 + 群组 @提及
- `WebAdapter`：基于 FastAPI，提供 REST API + SSE 流式响应

### 5.7 配置系统

```yaml
# xclaw.config.yaml

# LLM 配置
llm_provider: "anthropic"          # anthropic | openai | deepseek
api_key: ""
model: "claude-sonnet-4-20250514"
max_tokens: 4096
max_tool_iterations: 50

# 渠道
telegram_bot_token: ""
web_enabled: true
web_host: "127.0.0.1"
web_port: 8080

# 会话管理
max_session_messages: 40
compact_keep_recent: 20
max_history_messages: 50
memory_token_budget: 1500

# 数据
data_dir: "./xclaw.data"
timezone: "Asia/Shanghai"

# 投资配置
stock_market_default: "CN"         # CN | US | HK
stock_data_source: "akshare"       # akshare | yfinance

# 安全
control_chat_ids: []               # 管理员 chat ID 列表
bash_enabled: false                # 默认关闭 bash 工具
```

---

## 6. 投资助手专项功能

### 6.1 投资工具集

| 工具 | 功能 | 风险 |
|------|------|------|
| `stock_quote` | 获取实时/延迟行情（价格、涨跌幅、成交量） | Low |
| `stock_history` | 获取历史 K 线数据（日/周/月线） | Low |
| `stock_indicators` | 计算技术指标（MA/MACD/RSI/KDJ/BOLL） | Low |
| `stock_fundamentals` | 获取财务数据（营收、利润、PE/PB/ROE） | Low |
| `stock_news` | 获取个股/市场新闻摘要 | Low |
| `watchlist_manage` | 管理自选股列表（添加/删除/查看） | Low |
| `portfolio_manage` | 管理持仓记录（买入/卖出/查看盈亏） | Low |
| `market_overview` | 大盘指数、板块涨跌排名、北向资金 | Low |

### 6.2 stock_quote 工具示例

```python
class StockQuoteTool(Tool):
    @property
    def name(self) -> str:
        return "stock_quote"

    @property
    def description(self) -> str:
        return "获取股票实时行情，包括当前价格、涨跌幅、成交量等"

    @property
    def parameters(self) -> dict:
        return {
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "股票代码，如 '600519'（A股）或 'AAPL'（美股）"
                },
                "market": {
                    "type": "string",
                    "enum": ["CN", "US", "HK"],
                    "description": "市场，默认 CN（A股）"
                }
            },
            "required": ["symbol"]
        }

    async def execute(self, params: dict, context: ToolContext) -> ToolResult:
        symbol = params["symbol"]
        market = params.get("market", "CN")
        # 使用 akshare 或 yfinance 获取数据
        data = await get_stock_quote(symbol, market)
        return ToolResult(content=format_quote(data))
```

### 6.3 投资场景示例

**场景 1：盯盘助手**
```
用户: 帮我关注贵州茅台和宁德时代，每天收盘后发一下涨跌情况
Agent: 已将 600519（贵州茅台）和 300750（宁德时代）加入自选，
       并创建定时任务：每个交易日 15:05 发送收盘行情。
```

**场景 2：技术分析**
```
用户: 分析一下比亚迪最近的走势，用 MACD 和 RSI 判断一下
Agent: [调用 stock_history → stock_indicators]
       002594 比亚迪近 60 日分析：
       - MACD: DIF 上穿 DEA，金叉信号，柱状由负转正
       - RSI(14): 62.3，处于中性偏强区间
       - 建议关注 250 元支撑位...
```

**场景 3：持仓管理**
```
用户: 我上周买了 500 股中国平安，均价 48.5
Agent: 已记录：601318 中国平安 500 股 @ ¥48.5
       当前价 ¥50.2，浮盈 ¥850 (+3.51%)
```

### 6.4 数据源策略

| 市场 | 推荐数据源 | 备注 |
|------|-----------|------|
| A 股 | akshare | 免费、覆盖全面、社区活跃 |
| 美股 | yfinance | 免费、实时延迟 15 分钟 |
| 港股 | akshare / yfinance | akshare 覆盖港股通 |
| 指数 | akshare | 上证、深证、创业板等 |
| 财务 | akshare | A 股财报、分红等 |

---

## 7. 任务处理系统

### 7.1 通用工具集（参照 MicroClaw）

| 工具 | 功能 | 优先级 |
|------|------|--------|
| `web_search` | 网络搜索 (DuckDuckGo) | P0 |
| `web_fetch` | URL 内容抓取 + HTML 清理 | P0 |
| `read_file` | 文件读取 | P0 |
| `write_file` | 文件写入 | P1 |
| `bash` | Shell 命令执行（可选、默认关闭） | P2 |
| `read_memory` | 读取记忆文件 | P0 |
| `write_memory` | 写入记忆文件 | P0 |
| `structured_memory_read` | 查询结构化记忆 | P1 |
| `structured_memory_update` | 更新结构化记忆 | P1 |
| `schedule_task` | 创建定时任务 | P0 |
| `list_scheduled_tasks` | 查看任务列表 | P0 |
| `cancel_scheduled_task` | 取消任务 | P1 |
| `sub_agent` | 子代理（受限工具集） | P2 |
| `export_chat` | 导出对话记录 | P2 |

### 7.2 定时任务调度

```python
# 使用 APScheduler 实现

class TaskScheduler:
    """后台任务调度器"""

    async def add_cron_task(self, task: ScheduledTask) -> None:
        """添加 cron 任务（如每日盘后分析）"""
        ...

    async def add_one_time_task(self, task: ScheduledTask, run_at: datetime) -> None:
        """添加一次性任务"""
        ...

    async def execute_task(self, task: ScheduledTask) -> None:
        """执行任务：调用 agent_loop 处理 task.prompt"""
        result = await agent_loop(context, task.prompt)
        await channel.send_response(task.chat_id, result)
```

---

## 8. 安全设计

### 8.1 基础安全措施

| 措施 | 说明 |
|------|------|
| **路径守卫** | 拦截对 `.ssh`, `.env`, `.aws`, `credentials` 等敏感路径的访问 |
| **工具风险分级** | Low / Medium / High 三级，High 级工具需管理员确认 |
| **Bash 默认关闭** | 命令执行工具默认禁用，需显式在配置中开启 |
| **API Key 保护** | 配置文件不进 git，日志中脱敏处理 |
| **Web 绑定本地** | 默认 `127.0.0.1`，不暴露到公网 |
| **速率限制** | Web API 限流（每会话/每窗口） |
| **管理员控制** | `control_chat_ids` 限制高权限操作 |
| **SQL 参数化** | 所有数据库操作使用参数化查询 |

### 8.2 投资数据安全

- 持仓数据仅存本地 SQLite，不上传云端
- 无自动交易功能（仅分析辅助，不接入券商 API）
- 不缓存用户的资金/账户敏感信息
- 投资工具标记为 Low 风险（只读数据获取）

### 8.3 路径守卫示例

```python
BLOCKED_PATHS = [
    ".ssh", ".aws", ".env", ".git/config",
    "credentials", "secrets", "private_key",
    ".gnupg", ".config/gcloud", "id_rsa",
]

def check_path_safe(path: str) -> bool:
    """检查路径是否安全可访问"""
    normalized = os.path.normpath(path).lower()
    return not any(blocked in normalized for blocked in BLOCKED_PATHS)
```

---

## 9. 分阶段开发路线图

### Phase 1：核心骨架（2-3 周）

**目标**：最小可用系统，能通过 Telegram 对话并调用工具

- [ ] 项目脚手架搭建（pyproject.toml, 目录结构, 配置加载）
- [ ] LLM Provider 抽象 + Anthropic 实现
- [ ] Pydantic 消息类型系统（Message, ToolUse, ToolResult）
- [ ] 核心 Agent 循环（agent_loop）
- [ ] Tool 基类 + ToolRegistry
- [ ] 基础工具：`web_search`, `web_fetch`
- [ ] SQLite 数据库层（chats, messages, sessions）
- [ ] 会话持久化 + 恢复
- [ ] Telegram 渠道适配（私聊）
- [ ] YAML 配置加载 + CLI 入口 (`xclaw start`)
- [ ] 基础日志

**验收标准**：通过 Telegram 与 Agent 对话，Agent 能搜索网页并回答问题，重启后会话可恢复。

### Phase 2：投资工具 + 记忆（2-3 周）

**目标**：投资助手核心功能可用

- [ ] 投资工具集：`stock_quote`, `stock_history`, `stock_indicators`
- [ ] 投资工具集：`stock_fundamentals`, `market_overview`
- [ ] 自选股管理：`watchlist_manage`（watchlist 表）
- [ ] 持仓管理：`portfolio_manage`（portfolio 表）
- [ ] 文件记忆系统（AGENTS.md）
- [ ] 结构化记忆（memories 表 + CRUD 工具）
- [ ] 显式 "记住..." 快速路径
- [ ] 上下文压缩（超限摘要）
- [ ] 文件工具：`read_file`, `write_file`
- [ ] System Prompt 增强（注入记忆 + 投资人设）

**验收标准**：用户可查行情、管理自选股和持仓、Agent 能记住用户偏好。

### Phase 3：定时任务 + Web（2-3 周）

**目标**：自动化 + Web 交互界面

- [ ] 定时任务系统（APScheduler + scheduled_tasks 表）
- [ ] 调度工具：`schedule_task`, `list_scheduled_tasks`, `cancel_scheduled_task`
- [ ] 每日盘后自动推送（集成投资工具）
- [ ] FastAPI Web 后端（/api/chat, /api/sessions, /api/config）
- [ ] SSE 流式响应
- [ ] Web 前端（React，简化版 MicroClaw UI）
- [ ] 股票新闻工具：`stock_news`
- [ ] Telegram 群组支持（@提及响应）
- [ ] Token 用量统计（llm_usage 表）

**验收标准**：定时任务可自动推送盘后分析，Web 界面可正常对话。

### Phase 4：增强 + 打磨（2-3 周）

**目标**：生产可用的质量

- [ ] OpenAI-compatible Provider（支持 DeepSeek / Ollama）
- [ ] 记忆质量规则（去重、置信度、归档）
- [ ] 路径守卫 + 工具风险审计
- [ ] Bash 工具（可选，默认关闭）
- [ ] Sub-agent 工具（受限工具集）
- [ ] Web 认证（auth token）
- [ ] 速率限制
- [ ] 交互式 setup 向导 (`xclaw setup`)
- [ ] 错误处理 + 优雅降级
- [ ] 单元测试 + 集成测试
- [ ] 文档完善（README, 配置说明, 工具参考）

**验收标准**：系统稳定运行，安全措施到位，文档齐全。

### Phase 5：可选进阶（按需）

- [ ] Discord 渠道适配
- [ ] 语义记忆（embedding + 向量检索）
- [ ] MCP 工具联邦
- [ ] Skills 系统（可扩展技能包）
- [ ] 投资回测工具
- [ ] 多用户隔离
- [ ] Docker 部署方案

---

## 10. 目录结构参考

```
xclaw/
├── pyproject.toml              # 项目配置 + 依赖
├── xclaw.config.example.yaml   # 配置模板
├── README.md
├── AGENTS.md                   # Agent 友好参考文档
│
├── xclaw/                      # Python 包
│   ├── __init__.py
│   ├── __main__.py             # python -m xclaw 入口
│   ├── cli.py                  # CLI 命令 (start, setup, help)
│   ├── config.py               # 配置加载 (pydantic-settings)
│   ├── runtime.py              # AppState 初始化 + 启动编排
│   │
│   ├── agent_engine.py         # 核心 Agent 循环
│   ├── llm.py                  # LLM Provider 抽象 + 实现
│   ├── llm_types.py            # 消息/工具 Pydantic 类型
│   │
│   ├── db.py                   # SQLite 数据库层
│   ├── memory.py               # 文件记忆管理 (AGENTS.md)
│   ├── memory_quality.py       # 记忆质量规则
│   ├── scheduler.py            # 定时任务调度
│   │
│   ├── channels/               # 渠道适配
│   │   ├── __init__.py
│   │   ├── telegram.py
│   │   └── web.py              # FastAPI 路由
│   │
│   ├── tools/                  # 工具系统
│   │   ├── __init__.py         # Tool 基类 + ToolRegistry
│   │   ├── web_search.py
│   │   ├── web_fetch.py
│   │   ├── read_file.py
│   │   ├── write_file.py
│   │   ├── memory_tools.py     # read_memory + write_memory
│   │   ├── structured_memory.py
│   │   ├── schedule.py
│   │   ├── path_guard.py       # 路径安全检查
│   │   │
│   │   ├── stock_quote.py      # 实时行情
│   │   ├── stock_history.py    # 历史 K 线
│   │   ├── stock_indicators.py # 技术指标
│   │   ├── stock_fundamentals.py
│   │   ├── stock_news.py       # 新闻摘要
│   │   ├── watchlist.py        # 自选股管理
│   │   ├── portfolio.py        # 持仓管理
│   │   └── market_overview.py  # 大盘概览
│   │
│   └── utils/
│       ├── __init__.py
│       ├── text.py             # 文本工具（消息分割等）
│       └── logging.py          # 日志配置
│
├── web/                        # Web 前端 (React)
│   ├── src/
│   ├── package.json
│   └── vite.config.ts
│
├── tests/
│   ├── test_agent_engine.py
│   ├── test_llm.py
│   ├── test_tools.py
│   ├── test_db.py
│   ├── test_stock_tools.py
│   └── test_config.py
│
└── xclaw.data/                 # 运行时数据（git 忽略）
    ├── xclaw.db
    ├── logs/
    └── groups/
        ├── AGENTS.md           # 全局记忆
        └── {chat_id}/
            └── AGENTS.md       # 聊天记忆
```

---

## 11. 与 MicroClaw 功能对照

| 功能 | MicroClaw | XClaw | 状态 |
|------|-----------|-------|------|
| Agent 循环 (工具调用) | ✅ | ✅ | Phase 1 |
| 会话持久化 + 恢复 | ✅ | ✅ | Phase 1 |
| 上下文压缩 | ✅ | ✅ | Phase 2 |
| 文件记忆 (AGENTS.md) | ✅ | ✅ | Phase 2 |
| 结构化记忆 (SQLite) | ✅ | ✅ | Phase 2 |
| 记忆质量 (去重/归档) | ✅ | ✅ (简化) | Phase 4 |
| 语义记忆 (Embedding) | ✅ (可选) | 🔮 | Phase 5 |
| 定时任务 | ✅ | ✅ | Phase 3 |
| Telegram 适配 | ✅ | ✅ | Phase 1 |
| Discord 适配 | ✅ | 🔮 | Phase 5 |
| Web UI | ✅ | ✅ | Phase 3 |
| SSE 流式 | ✅ | ✅ | Phase 3 |
| Web 搜索 | ✅ | ✅ | Phase 1 |
| URL 抓取 | ✅ | ✅ | Phase 1 |
| 文件操作 | ✅ | ✅ | Phase 2 |
| Bash 执行 | ✅ | ✅ (默认关闭) | Phase 4 |
| 路径守卫 | ✅ | ✅ | Phase 4 |
| Sub-agent | ✅ | ✅ | Phase 4 |
| Skills 系统 | ✅ | 🔮 | Phase 5 |
| MCP 联邦 | ✅ | 🔮 | Phase 5 |
| 多 Provider | ✅ (18+) | ✅ (3+) | Phase 1/4 |
| 交互 Setup | ✅ | ✅ | Phase 4 |
| **股票行情** | ❌ | ✅ | **Phase 2** |
| **技术分析** | ❌ | ✅ | **Phase 2** |
| **自选股/持仓** | ❌ | ✅ | **Phase 2** |
| **盘后自动推送** | ❌ | ✅ | **Phase 3** |
| **市场概览** | ❌ | ✅ | **Phase 2** |

图例：✅ 包含 | 🔮 后续可选 | ❌ 不包含

---

## 附录：快速启动命令参考

```bash
# 安装
pip install xclaw

# 初始化配置
xclaw setup

# 启动
xclaw start

# 诊断
xclaw doctor
```

---

*本文档基于 MicroClaw (Rust) 架构设计，为 XClaw (Python) 投资助手提供完整开发蓝图。*
*核心原则：简单、实用、安全。*
