# 工作流

设计。一张工作流同时是画布上的图和一份可编程的脚本，规格都在本文。对照 OpenBot 的工作流画布（`/opt/sourcecode/openbot` 的 `packages/ui/src/features/workflows/` 与 `src/backend/workflows/`）、Claude Code 的 dynamic workflows（`guide/workflows/dynamic-workflows.md`），以及 oar 把 Claude 的 `local_workflow` 投影成一条 `workflow` 任务的方式。落点是 Codync 现在的 host 和四个客户端。类型、表、方法、文件和屏幕以这里为准。

一张工作流属于**一台电脑**。图是触发器、提示词、助手、条件、审批和输出，用带类型的边连起来。脚本是 host 保存的一段受限 JavaScript：bot 能读、能改，由 host 执行。桌面（macOS、Linux、Windows 同一套代码）画这张图，也能打开脚本。iPhone 和 Android 用步骤列表查看、运行和编辑图能表达的形状；脚本要到运行时才知道有多少步时，手机只改触发器并查看脚本。终端用同一套列表和大纲，没有画布。保存、校验、定时、webhook 和执行都在 host。

例程保持原样。例程是某个 bot 自己的一条指令加一组触发器（`host/src/routines/`，`docs/features/routines.md`）。工作流可以在同一次运行里叫到这台电脑上的多个 bot。两者不合并，不互相迁移。

## 要做成什么样

桌面打开某台电脑的工作流之后，主区域是点阵画布，不是聊天。空画布中间是「从触发器开始」和「添加触发器」。加上节点之后，从左到右是触发器、智能体、输出，中间用线连着。触发器卡片上可以选手动或定时（每天 09:00，并写出下次运行）。智能体卡片上选这台电脑的一个 bot，任务写在卡片里，模型名是这个 bot 当前的模型，只读。输出卡片的目标是通知。选中一条边或一个节点可以删。底部工具条：选择、平移、铅笔、便利贴、形状、文本、搜索、添加节点。左下角是缩放（25% 到 200%）、缩放以适应、小地图。小地图默认收起，点它的按钮才展开。右上角是历史和运行。历史里能看到「最新结果」和每一次运行（第 N 次、今天的时间、耗时）。画布标题上的菜单列出这台电脑的工作流，当前这份标出步骤数。版本历史从这里展开：当前画布，以及按时间记下的自动保存、运行之前和恢复之前。同一处还可以新建、重命名、创建副本、置顶和删除。

手机从 bot 列表进入「工作流」。列表里每一行是名称和最近一次状态。点进去是详情：立即运行、编辑、步骤（按图的阅读顺序排成竖列）、触发器、运行记录、删除、版本号和保存时间。再点进某一次运行，能看到每一步的完成状态和输出；两路助手汇合到一个输出时，输出里按 `Input 1`、`Input 2` 分栏。审批停在这一页上，就地同意或拒绝。长按一条工作流，把它固定到这台电脑的列表里，固定项出现在 bot 的上面。

四端看到的是同一张工作流、同一次运行、同一句文案。手机改不了的形状（分叉超过一个条件、有环、有画布批注），手机只改各步的设置，边和批注原样保存。脚本要在运行中展开时，手机不重排步骤。

## 落到 Codync 上的决定

这些决定和 OpenBot 不一样，实现时按这里，不按 OpenBot 的源码抄。

**助手步骤就是这个 bot 的一轮对话。** 新增 `Cmd::WorkflowStep`，走现有的 bot actor、独立会话和聊天记录。host 不拿 harness 的凭证去调任何模型厂商。因此没有 OpenBot 那种单独的 LLM 节点（它直接 `generateText`）。脚本里的 `agent()` 也是这一轮对话，不是另一条补全接口；schema 只校验 bot 已经写下的文本。助手卡片上的模型名来自 bot 自己的配置，用户换模型还是在 bot 的设置里换。

**条件是对输入文本的匹配，不叫模型。** OpenBot 的条件让模型回答 yes 或 no。Codync 第一版的条件只有 `contains`、`equals`、`regex`、`empty`。匹配走 yes 边，否则走 no 边。条件的输出就是它收到的输入，后面的步骤还能读到原文。以后若要「让某个 bot 判断」，再加一种 `ask`，那是新的一步，不改这四种的含义。

**第一版能跑的节点**是触发器（手动、定时、webhook）、提示词、助手、条件、审批、输出（通知，或写进某个 bot 的聊天）。**只画在画布上、运行时跳过的**是文本、便利贴、形状和手绘。图片、素材、消息触发器、发到外部频道或写成文件，第一版不进枚举、不进添加菜单、不进数据库的 `CHECK`。要加的时候走一次 schema 迁移，给 walker 加一条臂，给添加菜单加一行。

**一个工作流同时只有一次未结束的运行。** 定时到点时若已有运行，这次跳过，并把下一次拨到再下一个发生时间。webhook 在忙碌时记下 `busy`，不排队重放。

**环有次数上限。** 同一个节点在一次运行里最多执行 20 次（`MAX_PASSES`）。第 21 次被标成失败，原因是 `This step repeated too many times`，整次运行失败，不再调用 bot。OpenBot 不设上限；这里设上限，避免一张图把 bot 一直占着。

**工作流可以暂停。** 表上有 `enabled`，默认开。关掉之后定时不跑、webhook 在云端直接拒绝，正在进行的那一次跑完。没有这个开关的话，只要留下一个定时触发器，`keeps_awake` 就会一直挡住空闲睡眠，云端的 hook 也没有 `enabled` 可填。

**复制工作流时节点和边都换成新 id。** 幂等键按工作流隔离，换 id 是为了固定项、深链和本地草稿不会指到副本的旧节点。

**事件不进全局 rev。** 见下文「事件」。旧客户端遇到不认识的 `type` 会丢掉这条（iOS `HostClient.parseEvent` 的 `default` 返回 `nil`，桌面 `parseEvent` 的 `default` 返回 `null`，Android `MirrorEvent.decode` 的 `else` 是 `Ignored`）。工作流事件不带 `rev`，也不写进 `entries`。旧应用的 `since` 游标只跟着 bot 和 entry 走，不会因为中间插了一条工作流事件而回绕到 rev 0。

**推送仍然要有一个 bot。** `push::notify` 的签名是 `(hub, bot, from, title, body, kind)`，点开通知会打开这个 bot 的聊天。工作流在密封载荷里另加 `workflowId` 和 `runId`。旧应用忽略不认识的键，打开那个 bot。新应用看到这两个字段就打开对应的运行页。锚点 bot 的选法见「通知」。

**手机不画布。** 第一版手机没有平移缩放的节点编辑器。手机编辑的是大纲；画布上的坐标、手绘、便利贴和形状由桌面维护，手机保存时原样带回去。

**不引入图编辑库。** 桌面依赖里没有 React Flow、xyflow、Konva。画布自己做：相机、边和命中测试是纯函数，节点是 React 组件。手绘、便利贴和形状要自由摆在同一层上，套一个节点图库会和这层打架。

## 第一版与以后

第一版交付下面「文件清单」里的全部路径，以及这里列出的节点、运行、定时、webhook、审批、通知、版本历史、桌面画布和手机大纲。四端同一次改动一起进，不先做桌面再补手机。

以后再做，本文不给实现：

| 能力 | 现在不做的原因 |
|---|---|
| LLM 节点 | 没有一条允许的、用用户自己的密钥做普通补全的路径。语音金库里的密钥只服务通话。 |
| 图片、素材节点 | 运行要读文件和生成图片，第一版的端口只有 `text`。 |
| 消息触发器 | Codync 没有 OpenBot 那种「消息场所」模型。 |
| 输出到外部频道或文件 | 输出第一版是通知和 bot 聊天。 |
| 条件的 `ask` | 等助手步骤的会话路径稳定之后再加。 |
| 手机上的画布 | 大纲覆盖截图里的手机流程。 |
| 跨电脑的一张图 | 产品仍是一台电脑。别的电脑上的 bot 不进这张图的助手选择器。 |
| 脚本再调用另一张工作流 | 一次运行只冻一份脚本。嵌套会再开一张工作流的运行。 |
| 脚本里的 token 预算、worktree 隔离 | 用量仍从各 bot 自己的 harness 读。工作流不另记预算，也不为一步建 git worktree。 |
| 用 Node 或某个 bot 的 shell 跑脚本 | host 里用 `boa_engine`。脚本存在工作流这一行，不是用户仓库里的文件。 |

## 架构

```text
手机 / 桌面 / 终端
    渲染图和运行，发出意图
        │  POST /api/<method>    GET /events（另加一条不带 rev 的 workflow 事件）
        ▼
host/src/workflows/          图、脚本、版本、运行、走图或重放、定时、webhook
        │  Cmd::WorkflowStep
        ▼
host/src/agent/bot/          已有的队列、会话、回合
        │
        ▼
该 bot 的聊天里一条「Workflow · 标题」通知，以及这一轮的记录
```

host 拥有逻辑。客户端不保存图的权威副本，也不自己算下次运行。边能不能连，客户端用同一套纯函数先查，为的是松手时立刻拒绝；host 在保存时再查一遍，客户端的结果不作数。

一台电脑一个 host，工作流存在这个 host 的 SQLite 里。账户下有多台电脑时，每台电脑各有自己的列表。客户端用那台电脑的 `BotStore`（桌面 `store`，iOS `accounts.store(for:)`，Android 当前连接的 `AppStore`）去调方法。不在账户层合并。

和例程的边界：

| | 例程 | 工作流 |
|---|---|---|
| 归属 | 一个 bot | 一台电脑 |
| 存储 | `kv` 键 `routines.v1` 一整份 JSON | schema 7 的表。已经按没有脚本列迁过的库再加 schema 8 |
| 步骤 | 一条指令 | 一张图，同时有一份脚本 |
| 进 actor | `Cmd::Routine` | `Cmd::WorkflowStep` |
| 会话槽 | `routine-<run id>` | `workflow-<step id>` |
| 用户停掉 bot | 现有的 Stop 停掉当前回合 | 同一条 Stop。停掉的若是工作流步骤，这一步记 `stopped`，运行按走图结果收场 |

工作流不写入 `routines.v1`，也不为每个触发器偷偷建一条例程。定时用自己的到期索引。`routines::schedule` 是私有模块，下次发生时间用已经导出的 `routines::Trigger::next`。

## 程式与画布

画布和脚本是同一张工作流的两面，不是两个功能。人在桌面画图，bot 读和改脚本。host 保存同一份 `revision`。触发器（手动、定时、webhook）、暂停、密钥、防休眠仍按图上的触发器节点，不写进脚本。脚本是触发器开火之后的正文。

参考的两处各取一块，不整份搬进来。

Claude Code 的 dynamic workflows 取这些：编排本身不消耗模型；`meta` 是运行前就能读到的字面量；`agent` 的输出可以用 schema 交给下一步；`parallel` 是屏障，`pipeline` 是逐项流过几段；恢复时把脚本再跑一遍，已经完成的 `agent()` 直接交回缓存。判断留在 bot 的提示词里。顺序、前置条件、重试次数和停下来的规则留在 host。

不取这些：把触发词从 `workflow` 改成 `ultracode`（那是 Claude Code 自己的命令）；编排器再去当一轮模型；`Date.now`、`Math.random`、读文件、起进程、访问网络；失败的 `agent()` 变成 `null` 让脚本继续；一次运行 1000 次调用、一个 `parallel` 4096 项；`budget` 对象；`isolation: "worktree"`；脚本里再调用 `workflow()`。

oar 取这些：一次工作流运行是一条类型为 `workflow` 的任务，它协调几次 agent，而不是又一个 runtime。Claude 的 `local_workflow` 在 oar 里映射成 `workflow`（`packages/oar/src/runtimes/claude/tasks.ts`）。`workflow_progress` 在 oar 里保持原生，不进 SDK 的花费。Codync 不依赖 oar 这个 crate，不把 oar 的 session graph 写进 host。父子关系就是这次运行和它的步骤行，加上 bot 聊天里已有的 `workflow-step-*` 通知。运行页要能看到阶段名和每一步的标题，这是客户端读 `workflow` 事件，不是把 Claude 的 `workflow_progress` 原样存下来。

一条消息交给一个 bot，仍然是聊天，不必做成工作流。一条没有顺序、没有恢复、没有固定输出形状的指令，仍然是例程。工作流用在多步、并行或流水、中断后能接着跑、下一步要靠上一步的结构。

### 谁说了算

`workflows.source` 是 `graph` 或 `script`。运行行上的 `source` 是另一套，表示 `manual`、`schedule` 或 `webhook`。两个字段不要做成同一个枚举。

- 人在一张 `source == graph` 的画布上保存：保持 `graph`，host 按下面的生成规则重写 `script_text`。执行走 `walk.rs`。生成出来的脚本给人读、给 bot 改，不拿去再执行一遍。
- 人在一张 `source == script` 的画布上保存：只能改触发器、批注和笔画。`saveWorkflowGraph` 若发现图里多了提示词、助手、条件、审批或输出，整次拒绝，不写库，理由是 `This workflow is a script. Edit the script to change its steps.` 通过时不改 `script_text`，不改 `source`。
- bot 或脚本面板保存脚本：host 先解析。文本和库里的 `script_text` 完全一致，并且触发器没变：不增加 `revision`，不重排。能落进静态子集的，编译回正文节点，`source` 设为 `graph`，之后走 `walk.rs`。静态子集认不出、但领域和 AST 检查通过的，`source` 设为 `script`，之后走 `script.rs`。
- 两条执行器不同时跑同一次运行。运行开始时把当时的 `source` 抄进 `workflow_runs.exec`。

静态子集是画布已经能表达的形状，文法见下文。`parallel`、`pipeline`、循环、`phase`、`log`、箭头函数、`.map` 一旦出现，就是动态脚本。画布不再把正文拆成可拖的节点。

编译回图时，触发器保留 id、坐标、日程和 webhook 密钥。批注和笔画留在原地。正文节点（提示词、助手、条件、审批、输出）按脚本重建。脚本里 `// @node <id>` 写着的 id，若种类没变，就沿用；否则新建 id，并按列重排这些正文节点（第一列 x = 480，之后每列 +400，y 从 80 起）。触发器不要挪到 x = 80，否则会把人摆好的定时卡片推走。

两个触发器连到不同的正文节点时，一份脚本表达不了这条差别。这种图仍按走图运行。生成的脚本可以读。bot 保存一份改过的静态脚本时，所有触发器改连到脚本里第一个用到 `args.input` 的步骤；一个都没有，触发器就不连到正文。这是脚本保存之后的图，不是原来走图的形状。

### 脚本长什么样

存在 `workflows.script_text`，最长 65536 字节（UTF-8）。按字节计，不用 SQLite 的 `length()`，那是字符数。再长拒绝保存，理由是 `This script is too long`。语法或编码读不下来：`This script couldn't be read`。解释器是 `boa_engine`，纯 Rust，不起 Node，不把脚本交给 bot 的 shell。不要用 boa 的默认浏览器领域。领域里只有参数上的绑定，加上 `JSON`、`Object`、`Array`、`String`、`Number`、`Boolean`、`Promise`、`Error`、`TypeError`、`RangeError`、`Math`（删掉 `random`）、`parseInt`、`parseFloat`、`isNaN`、`isFinite`，以及 host 注入的 `match(text, pattern)`。`match` 用 `regex` crate，不放进参数列表，也不使用 JavaScript 的正则。没有 `Date`、计时器、`fetch`、`require`、`import`、`process`、`globalThis`、`Proxy`、`Reflect`、`WebAssembly`。

保存时用 boa 的解析器看 AST，不用正则扫源码。字符串里写出 `Date` 不算使用。出现下面任一节点就拒绝，理由是 `This script uses "{name}", which isn't available`：`import`、除 `meta` 和 default 以外的 `export`、`eval`、`Function`、`require`、`process`、`Deno`、`Bun`、`fetch`、`XMLHttpRequest`、`WebSocket`、`Date`、`setTimeout`、`setInterval`、`queueMicrotask`、`Math.random`、`crypto`、`globalThis`、`Proxy`、`Reflect`、`WebAssembly`、`RegExp`、正则字面量。自由变量必须是参数里的名字，或脚本自己声明的绑定，或上面允许的全局。`workflow`、`budget` 不注入，写了就按同一句拒绝。

```js
export const meta = {
  name: "Review the change",
  description: "Read the diff, then check each finding.",
  phases: [{ title: "Review" }, { title: "Check" }]
}

export default async function ({ agent, parallel, pipeline, phase, log, args, approval, output }) {
  phase("Review")
  const notes = await agent("bot_abc", "Review this.\n\n" + (args.input || ""), {
    label: "Review",
    phase: "Review",
    schema: {
      type: "object",
      properties: {
        findings: { type: "array", items: { type: "string" } }
      },
      required: ["findings"]
    }
  })
  phase("Check")
  return await parallel(notes.findings.map((item) => () =>
    agent("bot_abc", "Check this finding.\n\n" + item, { label: "Check", phase: "Check" })
  ))
}
```

`meta` 必须是对象字面量，保存时求值，不跑 default 函数。`name` 是 1 到 160 的字符串，`description` 最长 2000 个字符，`phases` 最多 20 项，每项只有 `title`（1 到 80 个字符）。缺 `name`、多了别的键、或字面量里有函数调用，拒绝：`The workflow meta must be a literal`。详情里的 `phases` 是这些标题组成的数组，客户端不解析 JavaScript。

脚本保存时，只有新脚本的 `meta.name` 和库里那份的 `meta.name` 不同，才用新名字覆盖标题。画布上的重命名不改 `script_text`，也不增加 `revision`。这样把脚本原样存回，不会盖掉刚刚改过的标题。下一次按图生成脚本时，`meta.name` 用当时的标题。

参数必须是这个对象模式，键只能是上面八个名字，顺序不限，可以少用，不能多。default 导出只能有这一个 async 函数。

注入的八个：

| 名字 | 作用 |
|---|---|
| `args` | `{ input: string }`。webhook 的正文，截断规则用现有的 hook 正文上限。手动和定时是空串。 |
| `agent(botId, prompt, options?)` | 返回 Promise，结算后是一轮 `Cmd::WorkflowStep`。字面量 `botId` 在保存时就要是这台电脑上未删除的 bot，不能是群。运行时算出来的 id 到调用时再查，找不到或空串：这一步 `failed`，`Choose a bot for this step`。`prompt` 必须是非空字符串，否则 `The step didn't include a prompt`。超过 32_000 就截断，步骤 `truncated = 1`，和走图的输入上限相同。`options.label` 最长 80，再长截断，缺省 `Step N`。`options.phase` 是阶段名，最长 80。`options.schema` 见下。没有 schema 时 Promise 的值是字符串；有 schema 时是解析后的对象。没有 `model` 参数，模型是这个 bot 自己的。 |
| `parallel(thunks)` | `thunks` 必须是数组，元素是函数，最多 64 个。不是数组：运行失败，`This parallel didn't get a list of steps`。host 调用每个 thunk，把其中还没有结果的 `agent()` 一起发出去，全部结算后再重放。返回值按数组下标，不按完成先后。同一个 bot 的调用进那个 bot 的队列，不另起进程。不同 bot 可以同时跑。其中一次失败，这一批里已经发出去的等它结算，运行先保持 `running`，这批收完再标 `failed`。不再发新的调用。不把失败变成 `null`。 |
| `pipeline(items, ...stages)` | `items` 必须是数组，最多 64 项，来自字面量或上一次 `agent()` 的返回值。不是数组：`The pipeline didn't get a list`。`stages` 是 1 到 8 个函数，签名是 `(value, index)`。第一段的 `value` 是数组里的那一项，后面每一段的 `value` 是上一段的返回值。第 i 项的第 k 段可以在第 i+1 项的第 k 段之前开始。失败规则和 `parallel` 相同。 |
| `phase(title)` | 记下当前阶段。运行页用它做分组。不单独占一步，不发按 token 的事件。 |
| `log(message)` | 运行页上的一行字。不进 bot 聊天，不推送。 |
| `approval(question)` | 和审批节点一样挂起。`question` 空则用 `Approve this step?`。同意之后返回 `true` 并继续。拒绝则这次运行 `stopped`。 |
| `output(spec)` | `spec.target` 是 `notify` 或 `chat`。`chat` 时 `spec.botId` 必填，规则同输出节点。`spec.text` 是字符串。推送和写聊天在脚本挂起或返回之后发出，不在解释器里面发。 |

`schema` 只接受这一小段 JSON Schema：`type` 为 `object`、`array`、`string`、`number`、`boolean`、`null`；`properties`、`required`、`items`、`enum`。写了 `properties` 时 `additionalProperties` 视为 `false`。别的关键字保存就拒绝：`This step's schema isn't supported`。运行时把 bot 这一步的最终文本当 JSON 解析。失败则在同一会话里再送一轮，提示词附上校验错误。再失败，这一步 `failed`，理由是 `The step didn't match the expected shape`。文本仍留在聊天里。host 不为此去调模型厂商。

一次运行里 `agent()` 最多 100 次。第 101 次让运行失败：`This run called for too many steps`。`parallel` 或 `pipeline` 超过 64 项：`Too many steps at once`。`pipeline` 超过 8 段：`Too many stages`。这是脚本运行的上限。`source == graph` 的走图仍用原来的 `MAX_PASSES`（同一节点 20 次），两套上限不要混用。schema 校验的那一次补问不计入这 100 次。

### 静态子集和生成的脚本

静态子集只认下面这些语句。别的写法只要通过领域和 AST 检查，就整段当成动态脚本，不在保存时失败。

- 语句前可以有一行 `// @node <id>`。解析时这个 id 能对上同一种类的旧节点就沿用。
- `const 名字 = "字符串"`：提示词节点，文本就是这个字符串。
- `const 名字 = await agent("botId", 文本, { label: "…" })`：助手节点。文本只能是字符串字面量、一个已声明的名字、`名字 + args.input`、`"…" + args.input`，或下面这种汇合。
- 汇合只认生成器写出的形状：`"Input 1:\n" + 名字 + "\n\nInput 2:\n" + 名字`，更多路就按 `Input 3` 继续。解析时还原成多条边，顺序和走图的 `position` 一致。
- `await approval("…")`：审批。同意之后，原来的文本变量继续往下传。返回值是 `true`，生成的脚本不使用这个返回值。
- `await output({ target: "notify", text })` 或 `{ target: "chat", botId: "…", text }`。`text` 是字符串字面量或已声明的名字。
- `if (条件) { … } else { … }`，可以有 `else if`。条件只能是 `名字.includes("…")`、`名字 === "…"`、`名字.trim() === ""`、`match(名字, "…")`。`empty` 在走图里是去掉空白后为空，所以生成 `trim`，不要生成 `=== ""`。
- `return 名字`、`return "…"` 或 `return args.input`。

没有循环，没有 `parallel`，没有 `pipeline`，没有 `phase`，没有 `log`，没有箭头函数。`match` 的模式必须是字符串字面量，编译规则和条件节点的 `regex` 相同。

画布生成脚本时格式固定：两个空格缩进、双引号、文件末尾一个换行。每一步前一行 `// @node` 写节点 id。`meta.name` 是标题，`description` 是空串，不写 `phases`。触发器不进函数体。只有一个手动触发器、一个助手、一个通知时生成：

```js
export const meta = {
  name: "Review",
  description: ""
}

export default async function ({ agent, args, output }) {
  // @node n_agent
  const step1 = await agent("bot_abc", "Review this.\n\n" + args.input, { label: "Review" })
  // @node n_out
  await output({ target: "notify", text: step1 })
  return step1
}
```

手动和定时的 `args.input` 已经是空串，不要再写 `|| ""`。提示词节点变成它自己的 `const` 字符串，下一步用这个名字拼接。条件节点变成 `if`。`contains` 生成 `.includes`，`equals` 生成 `===`，`empty` 生成 `.trim() === ""`，`regex` 生成 `match(名字, "…")`。bot id 用节点上的 `botId`。还没选 bot 的助手，生成的脚本里这一步用空串，保存图仍然允许，运行到它仍失败，理由是 `Choose a bot for this step`。没有下游的提示词仍然生成那句 `const`，解析时它没有出边。

往返测试用生成器自己的文法：图编译成脚本，再解析回图，触发器 id 和坐标不变，正文节点的种类、id、字段和边与原图一致。两个触发器连到不同正文节点的图不进这组往返，生成的文本只作阅读。这个测试不通过，就不能把脚本交给 bot。

### 动态脚本怎么跑

`host/src/workflows/script.rs`。一次运行开始时把 `script_text` 和 `revision` 拷进该次运行。运行中再保存，只影响下一次。正在跑的这一次继续用冻住的文本。这是 bot「能改工作流」的边界：它改的是下一轮的编排，不是已经开始的这一轮。恢复靠重放同一份冻住的文本，不靠改写它。

`agent`、`approval`、`output` 都返回 Promise。boa 负责脚本里的 `async` / `await`。host 每次重放新建一个领域，把已经完成的调用直接完成对应的 Promise，然后驱动任务队列，直到函数返回，或停在一个还没有结果的 host Promise 上。不要用抛异常代替 `await`。每次重放都是新领域，脚本改过的全局不会留到下一次。

`agent`、`approval`、`output` 按这次运行里的出现顺序编号，`node_id` 为 `call-N`（N 从 1 起）。第一次执行插入一行 `workflow_steps`，重试再插入更高的 `attempt`，不另编一个 N。缓存按这个序号对齐，不对提示词做哈希：脚本已经冻住，第 n 次 host 调用就是同一步。已经 `done` 的 `output` 重放时直接返回，不再推送、不再写聊天。已经同意的 `approval` 直接返回 `true`，不再插第二行。还停在 `approval` 的那一次，重放继续等，不新开一行。`phase` 和 `log` 不占序号。一次重放结束时，用这一遍观察到的笔记整表换掉 `workflow_run_notes` 里这一次运行的行，不在旧行后面追加。

缓存只看这一号 attempt 最大的那一行。它是 `done` 才交回输出。它是 `running` 或 `approval` 就继续用这一行，不再插入。它是 `failed` 或 `stopped` 时不再插入，运行按这一步收场，避免失败之后自动重放又开一轮。只有还没有行时才插入。停在新的 `agent()` 时，用这一行，放开锁，用现有的 `Cmd::WorkflowStep` 交给 bot。重试要再跑，就先自己插入一行 `running`，重放才会执行它；重试不 `send_cmd`。提示词就是脚本传入的那段，不再包一层 `Workflow:` 和 `task`。步骤行的 `input` 存这段提示词，`bot_id` 和 `label` 写在步骤行上。快照图里没有 `call-N` 这个节点，`begin` 不能去快照里找它。schema 对不上时，这一步保持 `running`，在同一会话里再送一轮，不新占 `call-N`，也不计入 100 次上限。附上去的文本是：

```text
The last reply didn't match the schema:
{error}

Reply with only the JSON value.
```

第二次仍对不上，这一步 `failed`，理由是 `The step didn't match the expected shape`。重放看到失败就不再调用 bot，运行失败。

`parallel` 在一次重放里调用每个 thunk，把还没有行的 `agent()` 收集起来一起发出。这一号已经 `failed` 或 `stopped` 时不再发出，运行按这一步收场。下一次重放这些 Promise 已经有结果，thunk 才能跑到下一次 host 调用。`pipeline` 同样，每次只交出已经具备输入的那些段。

`approval()` 第一次执行时插入 `approval` 步骤并推送，运行状态的算法和走图相同：还有别的步骤在 `running` 时，运行保持 `running`。同意之后返回 `true`。拒绝则 `stop_run`，运行 `stopped`，不再重放。`output()` 第一次执行时走输出节点的同一条副作用（先落定再通知，或先 `insert_entry_once` 再落定），然后 Promise 完成。步骤已经 `done` 的，重放不再做副作用。不要先放开锁再发推送、回头才写成 `done`，sweep 会再发一次。

脚本 `return` 之后，若没有未完成的步骤，运行 `done`。返回值写入 `workflow_runs.output`：字符串原样，对象用 `JSON.stringify`，没有 return 或 `undefined` 则写空串。走图的运行这一列留空。

未接住的异常让运行失败。有消息就用消息，截到 500 字；没有消息则 `This workflow's script didn't finish`。

一次重放里，两次把控制交还 host 之间最多 10 万步 JavaScript。交还是指停在还没有结果的 `agent`、`approval` 或 `output` 上。`phase`、`log`、空的 `parallel`、以及缓存命中都留在同一次重放里，不重置配额，否则 `while (true) { phase("x") }` 停不下来。下一次重放重新计数。用尽则运行失败：`This workflow's script didn't finish`。

一次重放放在 `spawn_blocking` 里，`std` 锁在那个线程上拿，不把 `MutexGuard` 带过 `.await`。等 bot 和等审批要先放开锁。推送在步骤已经写成 `done` 并提交之后再发，可以在放开锁之后。一次跑满 100 个已完成调用的重放必须落在上面的配额里。

停止、审批拒绝、用户在聊天里按 Stop，都不再重放。`retryWorkflowStep` 把运行改回 `running`，并为这一号插入更高的 `attempt`、状态 `running`，然后重放。重放看到这行 `running` 就执行它，不再插第二行。其他号上 attempt 最大且为 `done` 的调用仍走缓存。不删掉旧行。这个工作流另有打开的运行时，拒绝，理由仍是 `A run is already in progress`。`runWorkflowStep` 对 `source == script` 直接拒绝：`This workflow decides its steps when it runs`。

host 重启：冻住的脚本还在。`done` 的 `call-N` 重放时直接交回。`running` 且会话还在的助手只重发 `Cmd::WorkflowStep`，不平行再开一次重放。会话没了，这一步按现有的中断失败收场，运行失败。没有任何 `running` 或 `approval` 步骤的脚本运行，sweep 再重放一次，从缓存往下走。

`workflow_steps.label` 和 `phase` 给运行页做标题和分组。图执行的步骤这两列为空，标题仍从节点来。脚本步骤的标题用 `label`，空则 `Step N`。运行页按 `call-N` 的顺序排，不按图的拓扑。`exec == script` 的通知锚点是这次运行里第一个有 `bot_id` 的助手步骤；还没有时用 lead bot。没有 lead bot 则不推送，运行页上仍有结果。

### bot 怎么改

bot 不写 SQLite，也不写 `~/.codync` 下的文件。内置 MCP 增加四个工具，实现在 `host/src/workflows/tools.rs`，注册在现有的内置服务器旁边，调用和 HTTP 同一套函数。这是 bot 侧的工具，和例程的 `list_routines` 一样，不检查设备的 `Scope`。这台电脑上的任何一个 bot 都能读写这台电脑的工作流。HTTP 上的方法仍走 `devices::permit`。

| 工具 | 参数 | 结果 |
|---|---|---|
| `list_workflows` | 无 | 摘要数组，含 `source` |
| `read_workflow` | `{ id }` | 详情，含 `script`、`source`、`revision`、`graph` |
| `save_workflow` | `{ id, baseRevision, script?, triggers? }` | 和 `saveWorkflowScript` 相同。`conflict: true` 时带上当前详情，bot 自己决定要不要按新脚本再写。host 不合并两份文本。 |
| `run_workflow` | `{ id }` | `startWorkflowRun`。`requestId` 由 host 生成。 |

正在执行这张工作流的 bot 也可以保存它。保存进下一轮，不改本轮快照。`read_workflow` 走 `reveal: false`。工具结果里不要带 webhook 密钥；密钥仍只在 `reveal: true` 的 `workflow` 方法里出现。工作流已有打开的运行时，`run_workflow` 返回 `A run is already in progress`，不因为 host 新造了 `requestId` 就再开一轮。

### 画布和手机上怎么显示

桌面工具条加一个图标按钮，画的是一页纸中间写着 `</>`，tooltip 和辅助标签都是 `Script`。点开是侧栏里的文本框，不新增代码编辑器依赖。画布和脚本面板共用一个 `baseRevision`、一条在飞的保存。同一时刻整个工作流只有一次保存。飞着的时候再改只记脏标记，用响应里的新 `revision` 再存。一边保存成功，另一边还拿着旧文本，下一次保存会冲突。冲突则丢掉脏标记，用响应里的 `graph`、`script`、`phases` 和 `editMode` 换掉本地，一句 `This workflow was edited somewhere else`。`source == script` 时，触发器卡片上的日程和 webhook 走 `saveWorkflowScript`，只送 `triggers`。添加节点的菜单关掉。

`source == script` 时，画布留下触发器卡片、批注和笔画。正文收成一张卡片，标题用 `meta.name`，下面是 `meta.phases` 的标题，一句 `This workflow decides its steps when it runs`。这张卡片不能拉出连线。人要回到可拖的节点，就得把脚本改回静态子集再保存。

手机 `editMode` 增加 `script`。详情可以运行、停止、暂停、审批。触发器的表单仍可改，调用 `saveWorkflowScript`，只送 `triggers`，不送 `script`，避免把正文抹掉。步骤区在运行前是阶段标题；运行开始后按已经出现的步骤往下长。没有上移、下移、添加。顶部一句 `This workflow is a script. You can change its trigger. The steps appear when it runs.` 终端用同一句，脚本只读，触发器仍可改。

`phase` 变化发 `workflow` 事件，`change` 为 `phase`，带 `runId` 和 `phase`。不认识的 `change` 忽略，不要当成解不开的事件。脚本保存仍发 `change: graph`，因为图的投影和脚本一起变。本地有未保存的图或脚本时，不拿这条事件覆盖，下一次保存会冲突。

## 数据模型

时间全部是 Unix 毫秒，`store::now_ms()`，和例程一致。id 是字符串，用 host 里现有的 id 生成方式（和例程 id 同一处）。JSON 字段 `camelCase`。

### Rust 类型

放在 `host/src/workflows/model.rs`。结构体 `#[serde(rename_all = "camelCase")]`。下面这些枚举是单元变体，同样 `rename_all = "camelCase"`，线上是一个字符串，不带 `type` 标签：`"trigger"`、`"manual"`、`"yes"`、`"running"`。不要用相邻标签。日程是例外：它直接复用 `routines::Trigger`（`host/src/routines/mod.rs` 从私有的 `schedule` 模块导出），那种已经是 `#[serde(tag = "type")]`，所以 cron 仍是 `{"type":"cron",...}`。

```rust
pub enum NodeKind {
    Trigger,
    Prompt,
    Agent,
    Condition,
    Approval,
    Output,
    Text,
    Note,
    Shape,
}

pub enum PortType { Text }

pub enum Branch { Yes, No }

pub enum TriggerKind { Manual, Schedule, Webhook }

pub enum OutputTarget { Notify, Chat }

pub enum RunStatus { Running, Approval, Stopping, Done, Failed, Stopped }

pub enum StepStatus { Running, Approval, Done, Failed, Stopped, Skipped }

pub enum RunSource { Manual, Schedule, Webhook }

pub enum VersionKind { Auto, Restore, Run }

pub struct Point { pub x: f64, pub y: f64 }
pub struct Size { pub width: f64, pub height: f64 }

pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub at: Point,
    pub size: Option<Size>,
    pub locked: bool,
    pub fields: serde_json::Map<String, Value>,
    /// 只有定时触发器有。存的是编译后的 `Trigger::Cron`。
    pub schedule: Option<Trigger>,
}

pub struct Edge {
    pub id: String,
    pub from: String,
    pub to: String,
    /// 线上的键是 `type`。`type` 是 Rust 关键字，所以字段叫 `port`，`#[serde(rename = "type")]`。
    #[serde(rename = "type")]
    pub port: PortType,
    pub branch: Option<Branch>,
}

pub struct Stroke {
    pub id: String,
    pub brush: String, // "pen" | "marker"
    pub color: String,
    pub width: f64,
    pub points: Vec<Point>,
}

pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub strokes: Vec<Stroke>,
}
```

`fields` 是对象，不是任意 JSON。保存时按节点种类检查键，不认识的键拒绝，理由是 `unknown field`。这样旧 host 不会悄悄丢掉新应用写下的设置。

各种类 host 会读的键：

| 种类 | 键 | 约束 |
|---|---|---|
| `trigger` | `trigger` | `manual`、`schedule`、`webhook` |
| `trigger` | `timezone` | IANA 时区。定时时必填，用 `chrono_tz::Tz` 解析 |
| `prompt` | `text` | 字符串，可空 |
| `agent` | `botId` | 这台电脑上未删除的 bot。不能是群 |
| `agent` | `task` | 字符串。运行这一步时不能为空 |
| `condition` | `op` | `contains`、`equals`、`regex`、`empty`。前三种区分大小写 |
| `condition` | `value` | `empty` 时忽略。`regex` 用 `regex` crate（host 新增依赖，线性时间，不设超时），必须能编译，不带标志 |
| `approval` | `question` | 字符串，可空。空则用「Approve this step?」 |
| `output` | `target` | `notify` 或 `chat` |
| `output` | `botId` | `target == chat` 时必填，规则同助手的 `botId` |
| `text` | `text`、`font`、`align`、`bold` | 只给画布。`font` 是点数，8 到 96。`align` 是 `leading`、`center`、`trailing`。`bold` 是布尔 |
| `note` | `text`、`tone` | `tone` 是 `yellow`、`green`、`blue`、`violet`、`orange`、`gray`，和 `AVATAR_COLORS` 里已有的名字同一套 |
| `shape` | `shape`、`text`、`fill` | `shape` 是 `rectangle`、`ellipse`、`diamond`、`triangle`。`fill` 用和便利贴相同的六个名字 |

`schedule` 不放进 `fields`。它是节点上的 `schedule`，数据库列 `schedule_json`。只有 `fields.trigger == "schedule"` 时才能有它，手动和 webhook 必须是 `null`。编辑器拿现有的 `ScheduleDraft`（`host/src/routines/editor.rs`）编出五段 cron，预览走已经有的 `routineSchedule` 方法（`routines::schedule_preview`）。存下来的是 `{"type":"cron","expression":"0 9 * * *","timeZone":"Asia/Taipei"}`。间隔、一次性、事件触发器不属于工作流。用户在表单里只能选和例程相同的 Hourly、Daily、Weekdays、Weekly、Custom。

批注节点（`text`、`note`、`shape`）和 `strokes` 不产生步骤行。走图时看不见它们。

### 端口

```rust
pub fn ports(kind: NodeKind) -> (&'static [PortType], &'static [PortType]) {
    match kind {
        NodeKind::Trigger | NodeKind::Prompt => (&[], &[PortType::Text]),
        NodeKind::Agent | NodeKind::Condition | NodeKind::Approval => (&[PortType::Text], &[PortType::Text]),
        NodeKind::Output => (&[PortType::Text], &[]),
        NodeKind::Text | NodeKind::Note | NodeKind::Shape => (&[], &[]),
    }
}
```

条件的出边必须带 `branch`（`yes` 或 `no`），同一条件的同一侧最多一条出边。其他节点的边不能带 `branch`。禁止自己连自己。禁止重复边（同一 `from`、`to`、`branch`）。批注不能作为边的端点。端口类型第一版只有 `text`，类型不符在保存时拒绝。环是允许的，由次数上限兜住。

### 线协议里的图

`saveWorkflowGraph` 的 `graph`：

```json
{
  "nodes": [{
    "id": "n1",
    "kind": "agent",
    "at": { "x": 420, "y": 180 },
    "size": { "width": 350, "height": 220 },
    "locked": false,
    "fields": { "botId": "bot_1", "task": "Summarize the input." }
  }],
  "edges": [{
    "id": "e1",
    "from": "n1",
    "to": "n2",
    "type": "text"
  }],
  "strokes": [{
    "id": "s1",
    "brush": "pen",
    "color": "#1c1c1e",
    "width": 2.5,
    "points": [{ "x": 10, "y": 12 }, { "x": 14, "y": 18 }]
  }]
}
```

条件边上多一个 `"branch": "yes"`。定时触发器多一个 `"schedule"`。

### 上限

和 OpenBot 的量级对齐，写在 `host/src/workflows/limits.rs`，保存和运行共用。

| 限制 | 值 |
|---|---|
| 节点 | 200 |
| 边 | 400 |
| 笔画 | 500 |
| 每笔点数 | 2000 |
| 每个节点的 field 键 | 32，键名最长 64 |
| field 字符串 | 20000 |
| 标题 | 去掉首尾空白后 1 到 160 |
| 整图 JSON | 1 MiB |
| 坐标 | 绝对值不超过 1_000_000 |
| 节点宽高、笔画宽度 | 不超过 100_000 |
| 合并后的步骤输入 | 32_000，超出截断。步骤 JSON 里 `truncated` 是布尔；表里的列是 `INTEGER` 0 或 1 |
| 步骤输出、错误，经事件或详情返回时 | 32_000 |
| webhook 正文 | 64 KiB（`routines::hooks::MAX_BODY`） |
| 运行列表一页 | 1 到 50，客户端用 20 |
| 版本 | 每个工作流保留最近 50 条 |
| 一次运行带回的步骤 | 最新 2000 条 |
| 撤销栈（只在桌面画布内存里） | 100 |

超限的保存返回错误字符串，不写库。

### 表

`host/src/store/schema.rs` 的 `SCHEMA` 从 6 升到 7。`migrate_7` 只建表，不改 `entries`。现有迁移函数的写法是 `c.execute_batch` 包在 `BEGIN` / `COMMIT` 里，沿用。

```sql
CREATE TABLE workflows (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL CHECK(length(title) BETWEEN 1 AND 160),
  revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
  enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
  strokes_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(strokes_json)),
  next_run_number INTEGER NOT NULL DEFAULT 1 CHECK(next_run_number >= 1),
  source TEXT NOT NULL DEFAULT 'graph' CHECK(source IN ('graph','script')),
  script_text TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE workflow_nodes (
  workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
  node_id TEXT NOT NULL,
  position INTEGER NOT NULL CHECK(position >= 0),
  kind TEXT NOT NULL CHECK(kind IN (
    'trigger','prompt','agent','condition','approval','output','text','note','shape')),
  bot_id TEXT,
  x REAL NOT NULL,
  y REAL NOT NULL,
  width REAL,
  height REAL,
  locked INTEGER NOT NULL DEFAULT 0 CHECK(locked IN (0, 1)),
  fields_json TEXT NOT NULL CHECK(json_valid(fields_json)),
  schedule_json TEXT CHECK(schedule_json IS NULL OR json_valid(schedule_json)),
  PRIMARY KEY(workflow_id, node_id)
);
CREATE INDEX workflow_nodes_bot ON workflow_nodes(bot_id) WHERE bot_id IS NOT NULL;
CREATE TABLE workflow_edges (
  workflow_id TEXT NOT NULL,
  edge_id TEXT NOT NULL,
  position INTEGER NOT NULL CHECK(position >= 0),
  from_node_id TEXT NOT NULL,
  to_node_id TEXT NOT NULL,
  port_type TEXT NOT NULL CHECK(port_type IN ('text')),
  branch TEXT CHECK(branch IS NULL OR branch IN ('yes','no')),
  CHECK(from_node_id <> to_node_id),
  PRIMARY KEY(workflow_id, edge_id),
  FOREIGN KEY(workflow_id, from_node_id) REFERENCES workflow_nodes(workflow_id, node_id) ON DELETE CASCADE,
  FOREIGN KEY(workflow_id, to_node_id) REFERENCES workflow_nodes(workflow_id, node_id) ON DELETE CASCADE
);
CREATE INDEX workflow_edges_from ON workflow_edges(workflow_id, from_node_id);
CREATE INDEX workflow_edges_to ON workflow_edges(workflow_id, to_node_id);
CREATE TABLE workflow_versions (
  id TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
  revision INTEGER NOT NULL CHECK(revision >= 0),
  kind TEXT NOT NULL CHECK(kind IN ('auto','restore','run')),
  graph_json TEXT NOT NULL CHECK(json_valid(graph_json)),
  script_text TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);
CREATE INDEX workflow_versions_recent ON workflow_versions(workflow_id, created_at DESC, id);
CREATE TABLE workflow_triggers (
  workflow_id TEXT NOT NULL,
  node_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('manual','schedule','webhook')),
  timezone TEXT,
  schedule_json TEXT CHECK(schedule_json IS NULL OR json_valid(schedule_json)),
  next_due_at INTEGER,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(workflow_id, node_id),
  FOREIGN KEY(workflow_id, node_id) REFERENCES workflow_nodes(workflow_id, node_id) ON DELETE CASCADE
);
CREATE INDEX workflow_triggers_due ON workflow_triggers(next_due_at) WHERE next_due_at IS NOT NULL;
CREATE TABLE workflow_webhook_secrets (
  workflow_id TEXT NOT NULL,
  node_id TEXT NOT NULL,
  hook_id TEXT NOT NULL UNIQUE,
  secret TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(workflow_id, node_id),
  FOREIGN KEY(workflow_id, node_id) REFERENCES workflow_triggers(workflow_id, node_id) ON DELETE CASCADE
);
CREATE TABLE workflow_webhook_receipts (
  id TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
  node_id TEXT NOT NULL,
  delivery_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('started','ignored')),
  reason TEXT CHECK(reason IS NULL OR reason IN ('busy','refused')),
  run_id TEXT,
  received_at INTEGER NOT NULL,
  UNIQUE(workflow_id, node_id, delivery_id)
);
CREATE TABLE workflow_runs (
  id TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
  number INTEGER NOT NULL CHECK(number >= 1),
  trigger_node_id TEXT,
  from_node_id TEXT,
  source TEXT NOT NULL CHECK(source IN ('manual','schedule','webhook')),
  scheduled_for INTEGER,
  idempotency_key TEXT NOT NULL,
  revision INTEGER NOT NULL CHECK(revision >= 0),
  exec TEXT NOT NULL DEFAULT 'graph' CHECK(exec IN ('graph','script')),
  graph_json TEXT NOT NULL CHECK(json_valid(graph_json)),
  script_text TEXT NOT NULL DEFAULT '',
  payload TEXT,
  status TEXT NOT NULL CHECK(status IN ('running','approval','stopping','done','failed','stopped')),
  error TEXT,
  output TEXT,
  started_at INTEGER NOT NULL,
  ended_at INTEGER,
  updated_at INTEGER NOT NULL,
  UNIQUE(workflow_id, number),
  UNIQUE(workflow_id, idempotency_key)
);
CREATE INDEX workflow_runs_recent ON workflow_runs(workflow_id, started_at DESC, id);
CREATE INDEX workflow_runs_open ON workflow_runs(status) WHERE status IN ('running','approval','stopping');
CREATE TABLE workflow_steps (
  id TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
  run_id TEXT REFERENCES workflow_runs(id) ON DELETE CASCADE,
  node_id TEXT NOT NULL,
  attempt INTEGER NOT NULL CHECK(attempt >= 1),
  request_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN (
    'prompt','agent','condition','approval','output','trigger')),
  bot_id TEXT,
  status TEXT NOT NULL CHECK(status IN ('running','approval','done','failed','stopped','skipped')),
  input TEXT NOT NULL DEFAULT '',
  output TEXT,
  label TEXT,
  phase TEXT,
  branch TEXT CHECK(branch IS NULL OR branch IN ('yes','no')),
  passes INTEGER NOT NULL DEFAULT 1 CHECK(passes >= 1),
  truncated INTEGER NOT NULL DEFAULT 0 CHECK(truncated IN (0, 1)),
  error TEXT,
  started_at INTEGER,
  ended_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(workflow_id, request_id)
);
CREATE UNIQUE INDEX workflow_steps_attempt
  ON workflow_steps(run_id, node_id, attempt) WHERE run_id IS NOT NULL;
CREATE INDEX workflow_steps_open ON workflow_steps(status) WHERE status IN ('running','approval');
CREATE TABLE workflow_run_notes (
  run_id TEXT NOT NULL,
  position INTEGER NOT NULL CHECK(position >= 0),
  kind TEXT NOT NULL CHECK(kind IN ('phase','log')),
  text TEXT NOT NULL,
  at INTEGER NOT NULL,
  PRIMARY KEY(run_id, position)
);
```

`source`、`script_text`、`exec`、运行上的 `output`、步骤上的 `label` 和 `phase`、以及 `workflow_run_notes` 和画布的表一起建。已经按没有这些列的 schema 7 迁过的库，用 schema 8 只加这些列和这张表，不改已有列。`script_text`、运行输出和笔记正文不进分析事件。

`position` 是编辑器送来的顺序，也是一个节点多条入边的输入顺序。相等时用 id 字典序断开，走图结果稳定。`bot_id` 从助手节点和「写进聊天」的输出节点冗余出来，删除 bot 时好找。`graph_json` 和运行行上的 `script_text` 是开始时的快照，运行中再编辑不影响这次运行。`exec` 记下这次用走图还是脚本重放。`hook_id` 是云端已经认识的那一个 hook id（`^[A-Za-z0-9_-]{1,64}$`，用带连字符的 UUID），不是工作流 id 加节点 id。

`Store::open` 的 pragma 没有 `foreign_keys`（`host/src/store/schema.rs`）。上面的 `REFERENCES` 和 `ON DELETE CASCADE` 不会生效。`remove` 和保存时删节点，都要按子表顺序自己删：回执、运行笔记、步骤、运行、版本、密钥、触发器、边、节点，最后才是工作流。不要为这一个功能去打开全局 `foreign_keys`。

`workflow_versions.kind`：`auto` 是保存时若距上一条版本已超过 5 分钟就记一条；`restore` 是恢复之前先把当前图记下来；`run` 是运行开始时，若最新版本的 `revision` 和这次运行的不同就记一条。超过 50 条时删最旧的。

写这些表用 `workflowTransaction`：没在事务里就 `BEGIN IMMEDIATE`，已经在事务里就跟着外层走。运行的「插入 run 行 + 复用步骤 + 第一次推进」必须在同一个事务里。

### 客户端上的同一份模型

手写，不生成代码。字段名和 host 的 JSON 一致。

| 端 | 文件 |
|---|---|
| 桌面 | `apps/desktop/src/renderer/views/workflows/workflow.ts` |
| iPhone | `apps/ios/Kit/Sources/CodyncKit/Models/Workflow.swift` |
| Android | `apps/android/core/src/main/kotlin/com/codync/android/core/Workflow.kt`（`@Serializable`） |
| 终端 | 直接用 host 的类型，不再抄一份 |

Swift 用 `Codable`，枚举原始值是上面的字符串。未知 `kind` 解码失败时，这一份详情标记为只读（`editMode = readOnly`），不把整次同步判成 `undecodable`。工作流不走 `HostEvent` 的 bot/entry 解码，所以失败不会把 rev 拨回 0。

## 保存、版本、删除、复制

`WorkflowStore` 在 `host/src/workflows/store.rs`。

`create(title, graph, script)`：标题去掉首尾空白，空则用 `Workflow`。`graph` 和 `script` 都省略时用一张图：一个手动触发器，放在 `{x: 80, y: 80}`，`source = graph`，并生成脚本。只给 `script` 时也先放上这个手动触发器，再按「程式与画布」解析脚本。同时给了图和脚本时，以脚本为准，触发器取图里的触发器；图里没有触发器就用那个默认的手动触发器。图先过 `validate`。脚本解析完、正文节点落定之后再 `validate` 一次。插入 `workflows` 行，`revision = 1`，`enabled = 1`，写下节点、边、笔画、触发器行和 `script_text`。`// @node` 里的 id 若不在这份图里，就丢掉，由解析新建。

`save(id, base_revision, graph)`：

1. 读当前 `revision`。不相等则返回 `Conflict(detail)`，不写。
2. `validate`。当前 `source == script` 时，图里若有提示词、助手、条件、审批或输出，返回 `This workflow is a script. Edit the script to change its steps.`，不写。通过则只更新触发器、批注和笔画，`script_text` 和 `source` 不动。
3. 当前 `source == graph` 时整图替换节点和边（按主键 upsert，库里有而图里没有的删掉），并按生成规则重写 `script_text`。
4. 同步 `workflow_triggers`。定时的 cron 和时区没变就留着原来的 `next_due_at`，变了就重算。触发器从 webhook 改成别的，或节点被删，删掉 `workflow_webhook_secrets` 里对应行。改成 webhook 且还没有密钥时，生成一个密钥。
5. `revision + 1`。距上一条 `workflow_versions` 超过 5 分钟就插入 `auto`，版本里同时记下 `graph_json` 和 `script_text`。
6. 发出 `workflow` 事件，`change` 为 `graph`。

`save_script` 的冲突规则和上面相同。文本与触发器都没变则不增加 `revision`。变了就按「程式与画布」解析，静态子集编译回正文并保留触发器 id 和密钥，否则 `source = script` 并删掉正文节点。版本同样记下图和脚本。

`restore(id, version_id)`：先把当前图和脚本记成 `restore` 版本，再写入那一版的图和 `script_text`，`revision + 1`。`script_text` 为空：当作 `graph`，按图重新生成脚本。非空则按保存脚本的规则重算 `source`。恢复也要过 `validate`。旧版本里若出现第一版不认识的种类，或脚本现在读不下来，拒绝恢复并返回 `This version uses a step this computer can't run`。

`remove(id)`：先把这个工作流上 `running` 或 `approval` 的运行标成 `stopping`，并取消还在 bot 队列里的步骤（见执行）。然后按子表顺序删掉回执、运行笔记、步骤、运行、版本、密钥、触发器、边、节点，最后删 `workflows` 这一行。事件 `change` 为 `deleted`。

复制不在 host 做。客户端 `create`，标题为 `Copy of {title}`（截到 160），图里的节点 id、边 id、笔画 id 全部新生成，边的 `from` / `to` 跟着映射。`source == graph` 时不送脚本，host 按新图生成。`source == script` 时把 `script` 原文一并送上，触发器用新 id。触发器的 webhook 密钥不复制，保存后由 host 发新的。

删除 bot 时（`store` 里现有的删除路径，在 `host/src/store/bots.rs` 完成删除的那一处）：把 `workflow_nodes.bot_id = 该 id` 的 `fields_json` 里的 `botId` 清掉，列 `bot_id` 置空。不删节点，不删历史步骤。`source == graph` 时接着按图重写脚本，避免脚本里留下这个 id。`source == script` 时不改脚本，跑到这个 id 再失败。画布上这张助手卡片变成未选择，运行到它会失败，原因是 `Choose a bot for this step`。

## 图的规则

`host/src/workflows/graph.rs`。`validate(graph) -> Result<()>` 收集第一个问题就返回，错误文案是给用户看的英文句子（客户端再 `tr`）。问题种类：`annotation`（边碰到批注）、`port`（方向或类型不对）、`branch`（条件边缺分支，或其他边多了分支）、`condition-branch`（同一侧第二条出边）、`duplicate`、`limit`、`field`、`schedule`。

客户端有同一套纯函数：桌面在 `workflow.ts`，iOS 在 `WorkflowOutline.swift`（校验和大纲放一起），Android 在 `WorkflowOutline.kt`。终端直接调用 host 的 `validate`。三份客户端函数和 host 用同一组夹具测（见测试）。host 是权威。

### 走图用的环

`loop_edges(graph) -> HashSet<EdgeId>`。从没有入边的节点开始做深度优先；一条边指向仍在栈上的节点，这条边是回边。回边不参与「上游是否都完成」的等待，只在源步骤比当前这次尝试更新时，把目标再标成到期（下一次循环）。`passes` 从 1 计。下一次会变成 21 时，不执行，步骤 `failed`。

### 手机大纲

`host/src/workflows/outline.rs`。桌面的同一份函数在 `workflow.ts`，iOS 在 `WorkflowOutline.swift`，Android 在 `WorkflowOutline.kt`。保存时 host 不要求图是大纲；大纲只决定手机编辑器的模式。

能被大纲表达的图：

```text
trigger → main… → condition ┬ yes… ┐
                             └ no…  ┴→ after…
```

规则：

- 恰好一个触发器。
- 至多一个条件。
- 参与连线的步骤种类只有触发器、提示词、助手、条件、审批、输出。
- 除了两路重新汇合的那一个节点，每个步骤只有一条入边。
- 输出没有出端口。边只从它前面的步骤连到它。它后面的步骤不从输出连出，而从输出前面的那个步骤继续连。
- 提示词没有入端口，不能有指向它的边。它的出边指向同一列里它后面的下一个非提示词步骤。列尾若是提示词，大纲问题是 `prompt-target`，保存仍允许，运行时这条提示词没有下游。
- 触发器到不了的步骤放进 `loose`。它们保留自己的边，直到用户把它们接上或删掉。
- 批注和笔画不进入大纲，保存时原样写回。

```rust
pub struct Outline {
    pub trigger: String,
    pub main: Vec<String>,
    pub condition: Option<String>,
    pub yes: Vec<String>,
    pub no: Vec<String>,
    pub after: Vec<String>,
    pub loose: Vec<String>,
}

pub enum EditMode {
    Outline,   // 可以增删移动
    Fields,    // 只能改设置
    Script,    // 只改触发器，正文是脚本
    ReadOnly,  // 有不认识的种类，保存会丢
}
```

`edit_mode(graph, source)`：有不认识的种类则 `ReadOnly`；`source == script` 则 `Script`；`outline_of(graph)` 成功则 `Outline`；否则 `Fields`。详情里的 `editMode` 用这四个字符串：`outline`、`fields`、`script`、`readOnly`。

手机在 `Outline` 模式下增删移动之后，用 `graph_from_outline` 重写这些步骤的边。边的生成和 OpenBot 的 `outlineEdges` 同一条规则，边 id 用 `{from}>{to}:text`，条件边用 `{from}.{yes|no}>{to}:text`：

- 从触发器出发，沿 `main` 走。遇到提示词先攒着，不给它入边。遇到下一个非提示词步骤时，把当前源和攒着的提示词都连到它。
- 输出不成为下一步的源。源仍是输出前面的那个步骤。
- 没有条件时到此为止。有条件时，`main` 走完的源和攒着的提示词都连到条件。
- `yes` 从条件的 `yes` 端口走，`no` 从 `no` 端口走，规则相同。
- `after` 非空时，yes 和 no 两边走完的源一起连进 `after` 的第一个非提示词步骤。`loose` 里原有的、两端都在 `loose` 的边保留，其余边丢掉。

坐标按列重排（触发器 x = 80，之后每列 +400，y 从 80 起，分支两列上下分开 220）。批注的坐标不动。`Fields` 模式只改 `fields` 和 `schedule`，`nodes[].at`、边和笔画保持打开时拿到的那一份。

阅读顺序 `reading_order`：拓扑序，同一层按 `at.x` 再按 `at.y`。`source == graph` 的详情页和运行页用这个顺序排竖列，不管编辑模式是大纲还是只改设置。`exec == script` 的运行页按 `call-N` 排，见「程式与画布」。

## 执行

`source == graph` 时，`host/src/workflows/walk.rs` 决定下一步，`host/src/workflows/run.rs` 执行。`source == script` 时走「程式与画布」里的重放，不进 `walk.rs`。`host/src/workflows/service.rs` 是对外的 `Workflows`。`Hub` 上加 `workflows: Workflows`，在 `routines::start` 旁边调用 `workflows::start`。脚本在两次 host 调用之间占用解释器，等 bot 的时候放开工作流的锁，和走图等助手回合一样。

全局一把 `std::sync::Mutex` 包住 sweep 和用户动作（开始、停止、重试、审批、一次脚本重放），避免同一次运行被推进两次。用 `LockExt::locked()`。本地 hook 路由和中继收包是同步的，也要拿这把锁，所以不用 `tokio::sync::Mutex`。助手回合本身在 bot actor 里，不占这把锁。`Hub::send_cmd` 只是往 `UnboundedSender<Cmd>` 里送一条，不会等 actor。sweep 持锁时可以 `send_cmd`，不能等这一步跑完。脚本重放在 `spawn_blocking` 里拿锁，不把 `MutexGuard` 带过 `.await`。actor 之后再拿这把锁调用 `settle`。锁的顺序是先这把锁，再 `emit_lock`（发事件或 `insert_entry_once`）。不要在持有 `emit_lock` 时再拿工作流这把锁。

### 开始一次运行

| 来源 | 入口 | 幂等键 |
|---|---|---|
| 手动 | `start_run { id, requestId, fromNodeId? }` | `manual:{requestId}` |
| 定时 | 到点 | `schedule:{nodeId}:{scheduledFor}` |
| webhook | 投递 | `webhook:{nodeId}:{deliveryId}` |

`requestId` 由客户端生成（UUID）。同一键再次调用返回已有的那次运行，不新开。

`begin`：

1. 手动运行时，图里没有触发器但已有步骤（提示词、助手、条件、审批、输出），就从没有入边的步骤开始。没有任何步骤，或定时、webhook、脚本运行却没有触发器，返回 `Add a trigger first`，不插入运行。空白画布可以保存，不能跑。
2. 工作流 `enabled == false`：手动运行仍然允许（和例程的试跑一样）。定时和 webhook 不开始。
3. 若这个工作流已有 `running`、`approval` 或 `stopping` 的运行，手动和定时返回 `A run is already in progress`。webhook 记一条 `ignored` / `busy` 的回执，不开始。
4. 在内存里抄下当前图。第 5、6 步的拒绝发生在插入之前。插入之后，推进只读 `workflow_runs.graph_json` 里的这一份。没有 `fromNodeId` 时，每个触发器节点都会作为一步执行，输出都是这次运行的 payload。两个触发器连到同一步，那一步就收到两段输入。
5. 算范围 `scope`：
   - 没有 `fromNodeId`：所有非批注节点。
   - `fromNodeId` 不在快照里：返回 `That step is not in this workflow`，不插入运行。
   - `fromNodeId` 是触发器：触发器之后能走到的节点；这些节点所依赖的、且不是别的触发器的上游，一并算进来。
   - `fromNodeId` 是中间节点：这个节点和它的下游。上游里最近一次 `done` 的步骤记为 `reused`，不再执行。最近一次按 `ended_at` 降序，再按步骤 id 降序，包括 `run_id` 为空的单步。抄 `output` 和 `branch`（条件不抄分支的话，yes/no 边都不会生效）。新行 `attempt = 1`，`request_id` 为 `{runId}:{nodeId}:reused`。
6. 范围里有缺失必填字段的节点（助手没有 `botId`，或 `task` 为空且没有入边；输出 `chat` 没有 `botId`；定时触发器没有合法 cron），拒绝开始，不插入运行。单步运行同样检查。入边带来的文字可以代替空的 `task`。
7. 插入运行，`status = running`，`number` 取 `next_run_number` 再自增。需要的话写 `run` 版本。
8. 插入复用步骤，然后 `advance`。

`source == script` 时不走第 4 到 8 步。1 到 3 步照旧（没有触发器、暂停、已有运行）。`fromNodeId` 不是快照里的触发器则返回 `That step is not in this workflow`，不插入。不检查正文节点的必填字段。定时触发器仍然要有合法 cron。插入运行时 `exec = script`，`graph_json` 和 `script_text` 都抄开始时的那一份。不插入触发器步骤。`args.input` 就是这次的 `payload`。然后重放。

单步运行 `run_step`：`source == script` 时拒绝，`This workflow decides its steps when it runs`。`source == graph` 时 `run_id` 为空，幂等键是客户端的 `requestId`，唯一约束是 `(workflow_id, request_id)`。输入来自这个节点当前入边的源节点在最近一次 `done` 步骤里的输出，最近一次的排序和上面相同，并带上 `branch`。条件、审批、输出在没有任何一段可复用输入时返回 `Run the previous step first`。触发器、提示词、助手不要求已有输入。整图正在跑时拒绝单步，返回 `A run is already in progress`。

### 推进

`advance(run_id)` 在锁内循环调用 `next_actions`，直到这一轮只剩下要等 bot 或要等人的步骤。

对拓扑序中的每个非批注节点（回边不参与排序；同一层按快照里的 `position`，再按节点 id）：

- 已有 `running` 或 `approval` 的步骤：等待。
- 还没有步骤，或上游有比当前尝试更新的 `done`，或一条回边的源步骤更新：到期。
- 到期，但有上游也到期或在等待：继续等。
- 收集入边。边生效的条件：源步骤 `done`，且若边有 `branch`，源步骤的 `branch` 与之相同。回边不作为这次的输入。
- 没有任何生效入边，但本来有入边：`skip`。
- 否则 `run`，输入是生效入边按 `position` 拼出来的文本。

多路输入拼法：只有一路时就是那段文本。多路时每路一段，空行分隔：

```text
Input 1:
{第一段}

Input 2:
{第二段}
```

顺序是边的 `position`。总长超过 32_000 就截断，`truncated = 1`。这和运行详情截图里的 `Input 1` / `Input 2` 一致。

### 每一种节点

| 种类 | 行为 |
|---|---|
| `trigger` | 立刻 `done`。输出是运行的 `payload`。手动和定时是空字符串。webhook 是正文，超过上限的部分不进入 payload，开始时就拒绝，原因 `invalid`。 |
| `prompt` | 立刻 `done`。输出是 `fields.text`。 |
| `condition` | 立刻算。`contains`：输入包含 `value`。`equals`：整段相等。`regex`：`Regex::new` 后 `is_match`。`empty`：去掉空白后为空则 yes。输出等于输入，`branch` 为 `yes` 或 `no`。正则编译失败在保存时已经拒绝，运行中不再发生。 |
| `agent` | 插入 `running` 步骤，`send_cmd(bot_id, Cmd::WorkflowStep { step_id })`。见下一节。 |
| `approval` | 这一步变成 `approval`，然后推送 `NeedsInput`。运行要等没有别的步骤还在 `running` 时才变成 `approval`。人点同意：步骤 `done`，输出等于输入。人点拒绝：`stop_run`，运行 `stopped`。 |
| `output` / `notify` | 先把步骤写成 `done`，输出等于输入，提交之后再推送 `Done`。标题是工作流标题，正文是输出。 |
| `output` / `chat` | 先 `insert_entry_once`，id 为 `workflow-output-{stepId}`。成功后再把步骤写成 `done`。bot 已删除则步骤 `failed`，不写 notice。`insert_entry_once` 自己会锁数据库并提交，不能包进工作流那次写入的事务里。 |

真正执行的步骤行，`request_id` 为 `{runId}:{nodeId}:{attempt}`。`attempt` 是这个节点已有尝试数加一。复用行用 `{runId}:{nodeId}:reused`，不走这个格式。

还有步骤开着时，运行状态这样定：有任意一步是 `running`，运行就是 `running`（旁边可以同时有一步停在 `approval`，运行页照样显示同意和拒绝）。开着的步骤全是 `approval` 时，运行才是 `approval`。

没有待完成步骤时收场：

- 运行是 `stopping`：`stopped`。
- 有步骤 `failed`：`failed`，`error` 取第一条失败的 `error`。
- 有步骤 `stopped` 且没有失败：`stopped`。
- 否则 `done`。

收场后，若工作流仍 `enabled` 且这条触发器还是定时，按 `Trigger::next` 重算 `next_due_at`。每一步结算和运行收场都发出 `workflow` 事件，见「事件」。

### 停止、重试、审批

`stop_run`：运行标 `stopping`。还在 `running` 的助手步骤，向那个 bot 发 `Cmd::Stop` 仅当它当前回合就是这个 `step_id`；若该步骤还排在队列里，从队列撤掉并向 `settle` 报 `stopped`。审批中的步骤改为 `stopped`。然后 `advance`。`exec == script` 时不再重放。

`stop_step`：把这一步记成 `stopped`，然后 `advance`。其他步骤还开着时运行不收场。

`retry_step`：这一步所属的运行若已结束，而这个工作流另有打开的运行，返回 `A run is already in progress`，不把旧运行重新打开。没有别的打开运行时，把这次运行改回 `running`，新的 `attempt` 从已有次数加一（复用行占了 attempt 1 的话，第一次真正重试是 2）。助手步骤按快照重新拼输入。同一节点已有 `running` 或 `approval` 时拒绝。`exec == script` 时不按快照拼输入：插入更高的 `attempt` 且状态为 `running`，然后重放。重放不再插第二行。缓存只看 attempt 最大的那一行，只有它是 `done` 才交回。

`decide(step_id, approved)`：步骤必须是 `approval`。`approved == false` 走 `stop_run`。

### 重启

触发器、提示词、条件直接写成 `done`，不经过 `running`。输出的 `done` 也在副作用提交之后才落定：通知是先 `done` 再推送；写进聊天是先 `insert_entry_once` 再 `done`。崩溃发生在步骤行出现之前时，下一拍会再走这一步。通知因此最多丢一次、不会发两次。聊天 notice 的 id 固定，再走一次也不会多一条。步骤行已经是 `done` 的，重启不重做。

`workflows::start` 的第一拍先 `sweep` 一次打开的运行：

- 助手步骤 `running`：重新 `send_cmd`。actor 用会话槽装回会话，装不回则这一步 `failed`，`error` 为 `interrupted`，不自动再开一轮。`exec == script` 时只重发这一步，不平行再重放。
- 审批步骤保持 `approval`。推送不补发。
- `stopping` 的运行把还开着的步骤收成 `stopped` 再收场。
- `exec == script` 且没有 `running` 或 `approval` 的步骤：再重放一次，从缓存往下走。

助手步骤的外部副作用和例程一样：会话装不回就停，不自动重放。

## 助手步骤与会话

`host/src/agent/bot/cmd.rs`：

```rust
pub enum Cmd {
    // …
    /// 工作流的一个助手节点。`step_id` 是 `workflow_steps.id`。
    WorkflowStep { step_id: String },
    // …
}

pub(super) enum Queued {
    // …
    WorkflowStep(String),
}
```

actor 的队列处理加一条臂，和 `Queued::Routine` 并列。`host/src/agent/bot/queue.rs` 里例程的路径是：`routines.begin` → 聊天里一条通知 → `Prepared { lane, prompt, timeout }` → `start_turn`。工作流步骤仿这条路径，单独一个函数 `workflows::agent::begin(hub, step_id) -> Result<Prepared>`，不调用 `routines.begin`。

`begin` 做这些事：

1. 读步骤。步骤不是 `running` 就返回 `Ok(None)`（已经被停掉）。`exec == graph` 时再读运行快照里的节点。`exec == script` 时节点 id 是 `call-N`，快照里没有它；提示词、bot 和标题都在步骤行上。
2. `botId` 缺失或 bot 已删除：`settle` 为 `failed`，`Choose a bot for this step`。群（`BotConfig::is_group`）同样失败。
3. 会话槽用 `store::run_session_key(bot_id, &format!("workflow-{step_id}"))`，和例程的 `run_session_key(bot, &session_slot(run))` 同一张 kv。运行收场后删掉这个键。删除 bot 时现有的 `lane.session.{id}@*` 清理会把它一起带走。
4. actor 记住当前回合的 `step_id`。用户或 `Cmd::Stop` 停掉的若是这个回合，`settle` 为 `stopped`。不要给 `Cmd::Stop` 加字段。
5. 用 `insert_entry_once` 在 bot 的主聊天插入通知，id 为 `workflow-step-{stepId}`，`kind` 为 `notice`。`insert_entry` 会自己生成 UUID，不能用它。`data`：

```json
{
  "workflowId": "…",
  "workflowTitle": "123",
  "runId": "…",
  "stepId": "…",
  "notice": "workflow"
}
```

正文是 `Workflow · {title}`。这一轮的过程和最终回复都在这个 bot 的聊天里，最终文本同时写在步骤的 `output` 上。用户回复那条结果会进 bot 的主会话，不回到工作流，和例程一样。

6. `exec == graph` 时提示词是：

```text
Workflow: {title}

{task}

{input}
```

`task` 为空在开始运行时已经拒绝。这里再查一次，空则失败。`exec == script` 时提示词就是步骤行上的 `input`，不再包这一层。空则失败，`The step didn't include a prompt`。

7. 超时用 3600 秒，和例程的默认一样。第一版不在节点上单设超时。
8. 回合的最终文本写回 `workflow_steps.output`，`settle` 为 `done`，然后 `Workflows::advance`。回合失败则 `failed`，`error` 是 actor 已经给人看的那句。最终回复仍是这个 bot 聊天里的一条普通 agent 消息，和工作流步骤上的 `output` 各存一份。不要再插一条结果 notice。

用户在聊天里按 Stop，而当前回合记着工作流的 `step_id`：actor 结束回合，`settle` 为 `stopped`。不要为工作流另做一套停止。

bot 正忙时，步骤排在它的队列里。工作流运行保持 `running`。不同工作流的步骤打到不同 bot 上可以并行；打到同一个 bot 上由 actor 串行，这是现有队列的行为。

删除工作流或停止运行时，若步骤还在 `Queued::WorkflowStep`，出队并 `settle(stopped)`。已经在跑的按 Stop 处理。

聊天通知是 entry，走正常的 rev。工作流自己的事件不带 rev。两边不要混。

## 定时、webhook、通知、防休眠

### 定时

`host/src/workflows/schedule.rs`。`workflows::start` 里 `tokio::time::interval(1s)`，`MissedTickBehavior::Skip`，和 `routines::runner::start` 相同。每一拍：

1. 只看 `enabled = 1` 的工作流。到期的定时触发器（`next_due_at <= now`）先把 `next_due_at` 拨到 `Trigger::next` 的再下一次，再尝试 `start_run`。错过的多个点合成一次，不补跑。
2. 工作流已有打开的运行：这次不开始，到期时间已经拨过了。
3. 调用 `hub.update_keep_awake()`。

`Hub::update_keep_awake` 今天是 `busy || self.routines.keeps_awake()`。改成再或上 `self.workflows.keeps_awake()`。`keeps_awake` 为真的条件：存在 `enabled = 1`、`kind = schedule` 且 `next_due_at` 非空的触发器。和例程一样，只挡住空闲睡眠；合盖和关机照旧。暂停后再打开时，若 `next_due_at` 已过，下一拍合成一次运行并拨到再下一次。

下次运行的文案用 `routineSchedule` 已经返回的摘要。客户端不自己用 cron 库算。

### webhook

云端已经有一套 hook，不是按路径区分例程和工作流。公开地址是 `POST /v1/hooks/{computerId}/{hookId}`（`cloud/src/routes/relay.ts` 的 `routineHook`）。`hookId` 要匹配 `cloud/src/hooks.ts` 的 `HOOK_ID`。host 把整份列表用一条 `{"t":"hooks","hooks":[...]}` 换上 Durable Object 里的表（`cloud/src/relay.ts` 的 `onHooks`，先删后插）。工作流若单独再发一条，会把例程的 hook 清掉。

因此：

- 每个 webhook 触发器有自己的 `hook_id`（UUID）和 `secret`。公开 URL 用这个 `hook_id`，本地 URL 是 `http://127.0.0.1:{port}/hooks/workflows/{hookId}`，加在 `host/src/api/http.rs` 里例程那条路由旁边。
- `host/src/remote/relay.rs` 的 `publish_hooks` 发送例程的 `hooks()` 和工作流的 hook 拼在一起的一份。`Session` 除了 `hub.routines.subscribe_hooks()`，再选 `workflows` 自己的 watch。任一份变了就重新发布这一份合并列表。
- `hook_item` 先按现有逻辑交给 `hub.routines.receive`。例程不认识这个 id 时再交给工作流。两边都不认识就按今天的未知 hook 拒绝。不要新开一条云端路由，`private-relay` 也不提供公开 webhook。

校验复用 `routines::hooks::authorized`。投递 id 复用 `routines::hooks::delivery_id`（`x-delivery-id`、`x-github-delivery`、`idempotency-key`）。头缺失时用正文的 SHA-256 十六进制。头在但不是 `delivery_id` 接受的样子（空、超过 200、含非图形字符）时返回它的 `Invalid`，不要改成哈希。同一 `(workflow, node, delivery)` 的回执已存在则返回当时的结果。`hook_id` 若和某条例程的 id 相同，云端会因重复 id 拒绝整份列表，这时另生成一个。

密钥用和例程 `webhook_key()` 相同的做法：两个 `Uuid::simple()` 拼成的十六进制，没有 `+` `/` `=`。`rotateWorkflowWebhook` 替换密钥并叫醒 hook 的 watch。详情默认不带密钥，`workflow` 的 `reveal = true` 才返回。日志里不打密钥。

工作流暂停时，合并列表里这条 hook 的 `enabled` 为 false。云端自己回 `409 paused`，不会进队列。本地这条路由同样 `409`，`retry` 为 false。忙碌是 `409` 且 `retry` 为 true，回执 `busy`，云端按现有退避再送。正文超过 `MAX_BODY`：`400`，回执 `ignored` / `refused`。未知 hook 或密钥不对：`401`，正文 `invalid webhook key or signature`，不区分原因。

### 通知

`push::notify` 在函数内部组装密封 JSON 和 APNs 的 `data`（`botId`、`computerId`、`ctx`）。调用方塞不进字段。加一个参数，例如 `link: Option<WorkflowLink>`，其中有 `workflowId`、`runId`。这两个 id 写进 APNs 的 `data`，和 `botId` 并列。点通知时手机读的是 `userInfo`，不是密封体。`PushAlert`（`apps/ios/Kit/Sources/CodyncKit/Models/PushAlert.swift`）只解 `title`、`subtitle`、`body`、`from`、`faces`，多出来的密封字段会被丢掉，不能靠它做跳转。`NotificationService` 用字典取 `botId`，多一个键不会让展示失败。

锚点：

| 情况 | 锚点 bot |
|---|---|
| 输出目标是 `chat` | 那个 `botId` |
| 输出目标是 `notify`，图里有助手 | 阅读顺序里第一个有 `botId` 的助手 |
| 走图，且图里没有助手（提示词直接到通知） | 这台电脑的 lead bot（`docs/features/lead-bot.md`）。没有 lead bot 则不推送，运行页上仍有结果 |
| `exec == script` | 不看图里有没有助手。这次运行里第一个有 `bot_id` 的助手步骤。还没有时用 lead bot。没有 lead bot 则不推送 |
| 审批 | 同上，优先第一个助手，否则 lead bot |

`AlertKind::Done` 用于输出和成功收场，`NeedsInput` 用于审批，`Failed` 用于运行失败。标题是工作流标题，正文是输出或问题，截到推送函数里已有的长度（标题 80、正文 400）。

手机在前台连着时，现有的 `hub.phone_connected()` 会抑制推送。此时列表和打开中的运行页从 `workflow` 事件刷新。

点开通知：新应用的 `userInfo` 里有 `workflowId` 就先清掉当前打开的 bot（iOS 把 `accounts.selection` 设为 `nil`，Android 把 `selectedBot` 设为 `nil`），再打开该工作流。有 `runId` 就进运行页，没有就停在详情。旧应用不读这两个键，落到锚点 bot 的聊天，聊天里有那条 `Workflow · 标题` 通知。

`push.rs` 里给聊天用的 `throttled((bot_id, kind))` 不要改。工作流另用一张表，键是 `(step_id, kind)`，间隔仍是 5 秒。同一 bot 上的两条工作流不会互相吞掉。

## API 与事件

方法加在 `host/src/api/workflows.rs`，`api::route` 里在 `bots::call` 之前匹配。名字是驼峰动词，和 `saveRoutine` 一样。权限走现有的 `devices::permit`：本地和持有 `Scope::Control` 的设备可以调用，不进 `LOCAL_ONLY`。配对中的调用者维持现状，一律拒绝。

未知方法今天是 404。旧应用不会调用这些名字。新应用看到 hello 里没有 `workflows: true` 就不显示入口。

### hello

`host/src/api/events.rs` 的 hello JSON 加 `"workflows": true`。`host/src/api/host.rs` 的 `hello` 方法同样加上。客户端把它当成可选布尔，缺省是 false。这是加字段，不提高 `MIN_APP`（现在是 `host/src/compat.rs` 里的 `"2.21.0"`）。

三端的 hello 类型各加一个可选字段：桌面 `HostEvent` 的 hello 分支（`apps/desktop/src/renderer/client/host-client.ts`），iOS hello 解码，Android `MirrorEvent.Hello`。缺字段等于这台电脑还没有工作流。

### 方法

请求和响应都是 JSON 对象。错误用现有的 `anyhow` 失败路径，`error` 字符串是英文句子。冲突例外，它是成功响应，因为客户端要用里面的整份详情重置编辑器。

| 方法 | 请求 | 响应 |
|---|---|---|
| `workflows` | `{}` | `{ "workflows": [Summary] }` 按 `updatedAt` 降序 |
| `setWorkflowEnabled` | `{ "id", "enabled": true }` | `{ "workflow": Summary }`。不增加 `revision`。发出 `change: enabled`，并叫醒 hook 的 watch |
| `workflow` | `{ "id", "reveal": false }` | `{ "workflow": Detail }` |
| `createWorkflow` | `{ "title", "graph"?, "script"? }` | `{ "workflow": Detail }`。只给 `script` 时按「程式与画布」解析；能进静态子集就落成图 |
| `renameWorkflow` | `{ "id", "title" }` | `{ "workflow": Summary }` |
| `deleteWorkflow` | `{ "id" }` | `{}` |
| `saveWorkflowGraph` | `{ "id", "baseRevision", "graph" }` | `{ "ok": true, "revision", "workflow" }` 或 `{ "ok": false, "conflict": true, "workflow" }`。`source` 为 `graph` 时重写 `script`。`source` 为 `script` 时只接受触发器、批注和笔画，见保存一节 |
| `saveWorkflowScript` | `{ "id", "baseRevision", "script"?, "triggers"? }` | 同上。`script` 与库中一致且触发器没变时不增加 `revision`。只送 `triggers` 时保留原文和 `source`。至少要有 `script` 或 `triggers` 之一 |
| `workflowVersions` | `{ "id" }` | `{ "versions": [Version] }` 新的在前 |
| `workflowVersion` | `{ "id", "versionId" }` | `{ "graph": Graph, "script": "…" }` |
| `restoreWorkflowVersion` | `{ "id", "versionId" }` | `{ "workflow": Detail }` |
| `startWorkflowRun` | `{ "id", "requestId", "fromNodeId"? }` | `{ "run": RunDetail }` |
| `runWorkflowStep` | `{ "id", "nodeId", "requestId" }` | `{ "step": Step }` |
| `stopWorkflowStep` | `{ "stepId" }` | `{}` |
| `stopWorkflowRun` | `{ "runId" }` | `{}` |
| `retryWorkflowStep` | `{ "stepId" }` | `{ "step": Step }` |
| `decideWorkflowStep` | `{ "stepId", "approved": true }` | `{}` |
| `workflowRuns` | `{ "id", "limit", "before"? }` | `{ "runs": [RunSummary], "hasMore": true }` |
| `workflowRun` | `{ "runId" }` | `{ "run": RunDetail }` |
| `rotateWorkflowWebhook` | `{ "id", "nodeId" }` | `{ "secret": "…" }` |

`Summary`：`id`、`title`、`revision`、`enabled`、`source`（`graph` 或 `script`）、`updatedAt`、`running`（打开的运行的 `RunStatus` 或空）、`lastRun`（`number`、`status`、`endedAt` 或空）。

`Detail`：`Summary` 加 `createdAt`、`graph`、`script`、`phases`（字符串数组，从 `meta` 抽出）、`editMode`、每个 webhook 触发器的 `url`。`reveal == true` 时才加 `secret`。`workflow` 事件和 bot 工具的结果里都不带 `secret`。工作流的 `source` 与运行的 `source` 不是同一个枚举。

`Version`：`id`、`revision`、`kind`、`createdAt`。

`RunSummary`：`id`、`number`、`source`、`status`、`startedAt`、`endedAt`、`error`。

`RunDetail`：`RunSummary` 加 `exec`、`graph`（快照）、`script`（快照）、`output`、`phase`（当前阶段，没有则空）、`notes`（`{ kind, text, at }`）、`steps`。

`Step`：表里的列，camelCase，加上 `title`。图上的步骤用文案一节的展示名。脚本步骤用 `label`，空则 `Step N`。

`before` 是上一页最后一条的 `startedAt` 加 `id`，避免同一毫秒翻页漏行。实现用 `(started_at, id) < (?, ?)`。

### 事件

live 广播，不进 `events_stream` 的 catch-up 数组，不带 `rev`：

```json
{
  "type": "workflow",
  "change": "graph",
  "workflowId": "…",
  "runId": null
}
```

`change` 是 `graph`、`enabled`、`run`、`step`、`phase` 或 `deleted`。`enabled` 带新的 `enabled`。`run` 带 `runId`。`step` 带 `runId`（单步运行为空）和 `stepId`。`phase` 带 `runId` 和 `phase`。每一步结算发 `step`，阶段变化发 `phase`，运行收场再发 `run`。不要按 token 发。不认识的 `change` 忽略，不要当成解不开。`hub.events` 的广播容量是 1024，步骤事件够用；客户端若 `Lagged`，走现有的重新同步。

业务行写在工作流自己的表里。事件只在 `emit_lock` 里 `events.send`，不写 `events` 表，不占用 rev。拿锁的顺序仍是先工作流的互斥，再 `emit_lock`。

客户端：

- 解析：iOS `HostClient.parseEvent` 加 `case "workflow"`，解不开就返回 `nil`，**不要**返回 `.undecodable`。桌面 `parseEvent` 解不开返回 `null`。Android 解不开返回 `Ignored`。
- 应用：桌面 `bot-sync.ts` 的 `apply`、iOS `BotStore+Sync.swift`、Android 的镜像应用处。`hello.workflows == true` 时，store 保持一份 `Summary` 列表，给固定行和列表用。`enabled`、`run`、`step` 只更新摘要和打开中的运行，不替换正在编辑的图。`graph` 在本地没有未保存改动时重拉详情；有未保存改动就先留着，下一次保存会得到 `conflict`。`deleted` 关掉编辑并显示 `This workflow was deleted`。不写进 bot 缓存，不 `bump(rev)`。
- 打开屏幕时总是先拉详情，不依赖错过的事件。事件流重连或 `Lagged` 之后拉一次 `workflows`，把固定行的状态补上。列表可以先用 store 里的摘要。
- 终端的事件匹配若把未知 `type` 当成错误，加一条忽略。工作流页打开时同样重拉。

`LOCAL_EVENTS`（`accessRequests`、`cloud`）不要加上 `workflow`。配对过的手机必须能收到，否则运行页不会自己更新。

### 分析

`docs/features/analytics.md` 的做法：只记行为，不记标题、任务、提示词、输入输出。在 `api::dispatch` 已有的观察点上，这些方法记方法名即可，不要另记正文。屏幕名见下文。

## 桌面画布

一个代码库覆盖 macOS、Linux、Windows。不要为平台分叉视图。样式遵守 `docs/guides/desktop-styles.md`：`Text`、`HStack`、`VStack`、主题色、有填充的块不加描边。节点的位置和相机变换是数据，用 `transform: translate(...) scale(...)` 写在元素上，这是例外；颜色、字号、间距、圆角走 class 和 token。文件超过约 500 行就先拆。

入口：

- 多台电脑时，`apps/desktop/src/renderer/views/ComputerRosterSection.tsx` 在 Extensions 按钮旁边加一个图标按钮，符号 `flowchart`，标签 `Workflows`。`point.3.connected.trianglepath.dotted` 已经用在 Connectors 上，不要再用来表示工作流。电脑不在线或 `store.mismatch` 时不显示，和 `openExtensions` 相同。
- 工作流、扩展和账号在左边的图标栏上，机器人列表底部不再放这一排。
- 点下去清空当前 bot 选择，主区域换成画布，绑定这台电脑的 store。再点一个 bot，回到那个 bot 的线程。画布不做成盖住线程的 sheet。

`ChatWindow` 的 detail 增加一种状态：`workflowComputerId: string | null`。有值时渲染 `WorkflowsView`，并把对应 store 放进现有的 store context。

### 文件

全部在 `apps/desktop/src/renderer/views/workflows/`。

| 文件 | 职责 |
|---|---|
| `workflow.ts` | 类型、`ports`、`validate`、`readingOrder`、`outlineOf`、`graphFromOutline`、`loopEdges`、`editMode`。无 React |
| `workflow.test.ts` | 上面这些函数 |
| `camera.ts` | 缩放、平移、适应、小地图坐标。无 React |
| `camera.test.ts` | |
| `edges.ts` | 贝塞尔路径、中点、命中。无 React |
| `edges.test.ts` | |
| `history.ts` | 撤销栈，上限 100。无 React |
| `clipboard.ts` | 内存剪贴板。系统剪贴板只放一个 UUID 键，MIME `application/x-codync-workflow-nodes` |
| `WorkflowsView.tsx` | 拉列表和详情，400 ms 防抖保存，冲突时用返回的图重置 |
| `WorkflowCanvas.tsx` | 工具、快捷键、空状态、把子组件拼起来 |
| `WorkflowNodes.tsx` | 每种节点的卡片。再长就按种类拆文件 |
| `WorkflowEdges.tsx` | SVG 层 |
| `WorkflowHud.tsx` | 顶栏：切换、运行、历史、版本 |
| `WorkflowAddMenu.tsx` | 添加节点 |
| `WorkflowMinimap.tsx` | |
| `WorkflowZoom.tsx` | |
| `workflows.css` | 点阵、节点、边。无静态内联样式 |
| `ScriptPanel.tsx` | 脚本文本框。保存走 `saveWorkflowScript`，单飞规则和画布相同。冲突时换掉本地脚本 |

保存管线在 `WorkflowsView`：本地图或脚本文本一变就记进撤销栈。停止输入 400 ms 后保存。画布和脚本面板共用一条在飞的保存，见「程式与画布」。它在飞时又有改动，就记一个脏标记，等这一次回来再用响应里的新 `revision` 存最新的那一份。不要让第二次请求带着旧的 `baseRevision` 出发，那样会收到冲突并把本地编辑盖掉。成功就把 `baseRevision` 换成响应里的 `revision`。`conflict: true` 时丢掉脏标记，用响应里的 `workflow` 替换本地的图、脚本、阶段和 `editMode`，撤销栈清空，顶栏一句 `This workflow was edited somewhere else`。不要接着把冲突前的本地图再存一遍。运行之前先把挂起的保存发出去并等它结束，保存失败就不开始。手机编辑器用同一条单飞规则。

`hello.workflows !== true` 时不渲染入口。方法返回未知方法时同样隐藏，并把这句话记到 `lastError`，不弹成通用失败。

### 相机

```ts
type Camera = { x: number; y: number; scale: number }

const MIN_SCALE = 0.25
const MAX_SCALE = 2
const ZOOM_STEP = 1.2
const SNAP = 8
```

世界坐标 = `(client - camera) / scale`。滚轮平移。捏合或按着 Ctrl / ⌘ 滚轮缩放，比率分别是 0.007 和 0.002，钳在 `MIN_SCALE` 和 `MAX_SCALE`。适应画布算出包住全部节点（没有节点就包住视口中心）的 scale 和位移，边距 64。动画 200 ms，ease-out；跨度大时 240 ms。`prefers-reduced-motion` 时直接跳。打开画布且用户还没动过相机时自动适应一次。

点阵是 CSS 背景，20×20，跟着相机平移，不跟着缩放密度走样（背景位置用相机的 x、y，背景尺寸用 `20 * scale`）。

拖节点、缩放节点、方向键微调都吸到 8 的倍数。方向键 8，按着 Shift 是 32。焦点在输入框里时方向键不移动节点。

### 工具和快捷键

焦点在画布里且不在输入框时：

| 键 | 作用 |
|---|---|
| V / H | 选择 / 平移 |
| P / M / E | 钢笔 / 记号笔 / 橡皮 |
| N / R | 便利贴 / 形状 |
| ⌘F | 搜索节点（按标题和任务文本） |
| ⌘A | 全选未锁定的节点 |
| ⌘/ | 隐藏或显示界面。隐藏后只留画布，右下角有「显示界面」 |
| ⌘+ / ⌘− / ⌘0 | 放大、缩小、100% |
| ⇧1 | 适应 |
| ⌘Enter | 运行整张图 |
| 按住空格拖动 | 临时平移 |
| ⌘Z / ⇧⌘Z | 撤销 / 重做 |
| ⌘C / ⌘X / ⌘V / ⌘D | 复制、剪切、粘贴、原位复制 |
| Delete / Backspace | 删除选中的未锁定节点，或选中的边 |

钢笔宽度 2.5，记号笔 14，橡皮半径 8，采样间距 1.5，单位是屏幕像素，存进图之前除以 scale。笔画进 `graph.strokes`，参与保存，不参与运行。

选择：单击选中，Shift 加选，空白处拖出框选。锁定的节点不能移动、不能删除，能复制。拖完一次记一条撤销。输入框的一串按键合并成一条撤销，停 400 ms 提交。

右键画布空白处弹出菜单：添加节点（在点击处打开添加菜单）、全选、搜索、缩放以适应、隐藏界面。点在节点、边或按钮上不弹出。

### 连线

每个可运行节点在卡片左右有端口。触发器和提示词只有右侧输出。输出只有左侧输入。条件右侧是上下两个端口，标 `Yes` 和 `No`。批注没有端口。

从输出端口拖到输入端口。松手在空白处时弹出添加菜单，新建的节点自动接上。单击输出端口再单击输入端口也能连，再点同一个端口取消。拒绝的情况在端口上用现有的错误色，不连线：同侧、自己连自己、类型不符、重复。形成环时允许连接，边的中点标 `Loop`。

边的 id 是 `{from}>{to}:text`，条件边是 `{from}.{yes|no}>{to}:text`。路径是水平方向的三次贝塞尔，控制点距离为两端水平距离的一半，最小 48。运行中的边用主题色的流动；已完成的边用完成色；其余用次要色。颜色来自 token，不写死十六进制，除了用户给笔画和便利贴选的颜色。

删边：选中后 Delete，或悬停时的断开按钮。

### 节点卡片

壳是统一的：种类名、卡片、选中时底部的结果、单选时浮出的工具条（复制、锁定、删除）。默认宽度：提示词 296，助手 350，条件 320，其余 280。可横向拖右边，触发器 240–480，条件 260–560，便利贴 160–480 宽、120–480 高。

| 卡片 | 内容 | 运行 |
|---|---|---|
| 触发器 | 手动 / 定时 / webhook 三选一。定时复用 `views/routines/RoutineScheduleFields.tsx` 和 `TimeZonePicker.tsx`。webhook 显示 URL，密钥默认遮住，显示、复制、更换（更换前用现有的 `Dialog` 确认） | 卡片上的按钮调用 `startWorkflowRun`，`fromNodeId` 为这个触发器 |
| 提示词 | 多行文本 | 无 |
| 助手 | bot 选择（这台电脑的 bot，不含群），任务多行文本，模型名来自 `bot.model`，没有就显示 backend 名 | `runWorkflowStep`。没有 bot 或任务为空时按钮不可用 |
| 条件 | op 的选择和 value。两侧出端口写 Yes / No | 单步运行需要已有输入 |
| 审批 | 问题文本。状态是 `approval` 时显示同意和拒绝 | 同意拒绝调用 `decideWorkflowStep` |
| 输出 | 通知或某个 bot 的聊天 | 单步，需要已有输入 |
| 文本、便利贴、形状 | 对应字段。便利贴和形状的颜色是 `yellow`、`green`、`blue`、`violet`、`orange`、`gray`，不取任意颜色 | 无 |

结果条：该节点在「正在查看的那次运行」里的最新步骤。`done` 显示输出，`failed` 显示错误和重试，`running` 显示已等待的秒数，`skipped` 显示 Skipped，`approval` 显示两个按钮。输出节点完成时，这段文字放在一块完成色底上，前面一个勾，最多四行。查看历史运行时卡片只读，不能改字段；顶栏说明 `Viewing run {number}`，旁边一个按钮回到最新。

添加菜单分三组，和截图一致：

- 输入：触发器、提示词
- 步骤：助手
- 流程：条件、审批、输出

文本、便利贴、形状只在底部工具条，不进这个菜单。第一版菜单里没有 LLM、图片、素材。

空画布：居中一块，标题 `Start with a trigger`，按钮 `Add trigger`。按钮插一个手动触发器在视口中心。桌面新建送一张空图，所以打开时没有触发器，点这个按钮才出现。不传图的创建（手机、终端）仍放一个手动触发器。

顶栏运行按钮和 ⌘Enter 调用 `startWorkflowRun`，不带 `fromNodeId`。运行中按钮变成停止，调用 `stopWorkflowRun`。旁边是进度 `{done}/{total}`，total 是这次快照里的非批注节点数，done 含 `done`、`failed`、`skipped`、`stopped`。顶栏还有暂停开关，调用 `setWorkflowEnabled`。暂停不取消正在跑的那一次。

历史菜单：`Latest results`，然后每一行 `Run {number}`、相对今天的时间、耗时。一页 20，底部 `Show older runs` 把 `limit` 加上 20 再请求。选中某次运行后，卡片显示那次的步骤。

版本菜单：来自 `workflowVersions`。`auto` 显示成保存时间，`restore` 显示成恢复前，`run` 显示成某次运行开始时。点一条就用 `workflowVersion` 的图盖住画布，只读，顶栏 `Make current` 调用 `restoreWorkflowVersion`。

删除工作流：菜单里的删除先 `Dialog` 确认。确认后 `deleteWorkflow`。不在桌面做撤销删除；图的撤销只覆盖画布编辑。

bot 列表变化时，助手卡片的名字跟着 store 里的 bot 更新。bot 被删除后卡片留下，选择器为空，副文案 `Choose a bot`。

## iPhone

系统 `NavigationStack` 推入，和 `BotListView`、线程一样用系统返回和系统大标题。这是 `docs/design/ui-conventions.md` 里已经写明的例外。不要自己做顶栏，否则返回键会和截图里的系统返回不一致。列表、详情、编辑、运行都是 push。步骤的字段编辑用 `.codyncSheet`。确认删除用 `.codyncDialog`。选择触发器类型、条件 op、输出目标用 `DropdownMenu`。长按用 `.contextActions`。不要手写下拉层。

入口在 `apps/ios/Views/ComputerSection.swift`。`extensions` 旁边加一个同样尺寸的按钮，符号 `flowchart`，无障碍标签 `Workflows`。`hello` 没有 `workflows` 或电脑不在线时传 `nil`，按钮不出现。`BotListView.computerHeading` 把闭包接上：`NavigationStack` 的 path 增加一种目的地。path 今天是 `[BotReference]`。不要把工作流塞进 `BotReference`。在 `BotListView` 上另放一个 `NavigationPath` 不现实（栈已经绑在 `BotReference` 上）。做法：电脑标题按钮设置 `@State var workflowsComputer: ComputerID?`，用 `navigationDestination(item:)` 推入 `WorkflowsView`。它和 bot 的 `navigationDestination(for: BotReference.self)` 共存。

固定：长按列表行，菜单 `Pin` / `Unpin`。存在本机 `UserDefaults`，键 `pinnedWorkflows.{computerId}`，值是 id 数组，最多 20。不同步。固定的行画在该电脑标题下面、bot 上面，不参与 bot 的拖动排序。点一行推入详情。桌面用 `prefs` 里同样的键，画在 `ComputerRosterSection` 的子节点顶部，行的样式靠近 bot 行但没有未读数，副文案是最近一次运行的状态。

### 文件

`apps/ios/Kit/Sources/CodyncUI/Workflows/`：

| 文件 | 职责 |
|---|---|
| `WorkflowsView.swift` | 列表。进行中的在前，其余按 `updatedAt` |
| `WorkflowDetailView.swift` | 立即运行、编辑、步骤、触发器、运行记录、删除、版本 |
| `WorkflowRunView.swift` | 一次运行的步骤、输出、审批、重试、停止 |
| `WorkflowEditorView.swift` | 大纲或仅字段 |
| `WorkflowStepSheet.swift` | 单个步骤的字段 |
| `WorkflowOutline.swift` | 纯函数，无 SwiftUI |

模型和方法在 `CodyncKit/Models/Workflow.swift` 和 `HostClient` 的 extension（放在 `CodyncKit/Client/`，方法名和 host 一致）。

列表行：名称，副文案是 `Running`、`Needs you`、`Done`、`Failed`、`Stopped` 或空。空列表：`No workflows yet` 和 `New workflow`。新建调用 `createWorkflow`，标题 `Workflow`，图里只有一个手动触发器，然后推入编辑。

详情，对照运行详情那些截图：

- 标题是工作流名。
- 主按钮 `Run now`，次按钮 `Edit`。已有运行时主按钮变成 `Stop`。
- 步骤区用 `readingOrder` 排成竖列。每行：种类图标、标题、一行摘要（助手是 bot 名，触发器是手动或下次运行，输出是通知或 bot 名）。
- 触发器区重复第一步触发器的摘要。多个触发器时每个一块。
- 运行记录显示最近一条，点进去是运行列表，再点是 `WorkflowRunView`。
- 底部 `Delete workflow`，破坏性，`.codyncDialog`。
- 最底一行 `Version {revision} · Saved {time}`。

运行页：顶上 `Run {number}`，副文案 `Done · {time} · {duration}` 或 `Running`、`Needs you`。每一步一块，状态在右侧。点 `Show output` 展开输出，再点 `Hide output`。审批步展开同意和拒绝。失败步有 `Retry`。运行中顶上有 `Stop`。

编辑：

- `Outline`：可以在列里上移、下移、复制、删除、在后面添加。添加页的种类顺序是助手、条件、审批、提示词、输出。条件已存在时不能再加，行上说明 `This workflow already has a condition`。列尾的提示词标 `This prompt doesn't feed a step`。
- `Fields`：顶部一句 `This workflow was arranged on a computer. You can edit each step's settings.` 步骤可点进 sheet，不能排序删除。
- `Script`：见「程式与画布」。只改触发器，不编辑脚本文本。
- `ReadOnly`：没有保存按钮，一句 `Update Codync on this computer to edit this workflow.`
- `Outline` 和 `Fields` 保存调用 `saveWorkflowGraph`。`Script` 只改触发器时调用 `saveWorkflowScript`，不送 `script`。单飞规则和桌面相同。冲突时用返回的详情替换，sheet 关掉，详情上显示 `This workflow was edited somewhere else`。不要在客户端再算一遍 `editMode`，用详情里的那一个。

触发器和输出的表单复用例程那一套控件的布局（`RoutineEditorView` 的日程和时区），不要复制一套 cron 计算。webhook 的 URL 和密钥用 `RoutineWebhookPanel` 的同样交互：复制、显示、更换前确认。

减少动态时，推入和 sheet 走 `Motion.reduced`。

深链：通知的 `userInfo` 里有 `workflowId` 时，先把 `accounts.selection` 设为 `nil`（聊天盖住列表时，工作流推不出来），tab 切到 Bots，再设 `workflowsComputer` 和待打开的 `runId`。`WorkflowsView` 出现后推入详情，有 `runId` 再推入运行。处理一次就清掉。

详情顶上有暂停开关，调用 `setWorkflowEnabled`。暂停时定时和 webhook 停，正在跑的那一次不停。

## Android

和 iPhone 同一批行为、同一套文案、同一种空状态和错误。布局按 Compose 和 `apps/android/design/` 的组件，不用 Material 的按钮、开关、对话框。`CodyncSheet`、`CodyncDialog`、`DropdownMenu`、`contextActions`。

模型在 `apps/android/core/src/main/kotlin/com/codync/android/core/Workflow.kt`。调用加在 `AppStore` 现有的 host 调用处（例程是 `AppStoreBots.kt` 的 `loadRoutines`），新文件 `AppStoreWorkflows.kt`，避免把 `AppStoreBots.kt` 再撑大。

屏幕在 `apps/android/app/src/main/kotlin/com/codync/android/`：

| 文件 | 对应 |
|---|---|
| `WorkflowsView.kt` | `WorkflowsView.swift` |
| `WorkflowDetailView.kt` | 详情 |
| `WorkflowRunView.kt` | 运行 |
| `WorkflowEditorView.kt` | 编辑 |
| `WorkflowStepSheet.kt` | 字段 sheet |
| `WorkflowOutline.kt` | 纯函数 |

导航：`MainTabs.kt` 的 `NavStack` 今天只有 `"roster"` 和 `selectedBot` 两项，没有通用页面栈。在 `AppState` 上加工作流路由（打开的工作流 id，以及可选的 `runId`），作为 `NavStack` 里插在花名册和聊天之间的一项。打开工作流时把 `selectedBot` 清掉。不要新开 tab。顶栏在 `NewMenuButton` 左侧加 `flowchart`，点击条件与 iOS 相同（在线且 hello 声明了 workflows）。`MainTabs.kt` 的 tab 保持 Bots、State、Settings 三个。详情上的暂停开关和 iPhone 相同。

Android 的 bot 列表今天不按电脑分段。固定的工作流画在列表顶部、bot 之前。`ComputersView.kt` 里每台电脑已有 Extensions 行的，旁边加一行 `Workflows`，只在那台电脑就是当前连接时能点，否则副文案用现有的离线句子。

通知点开走和 iOS 相同的字段。`MainActivity` 里处理 deep link 的地方加上 `workflowId`。

单元测试：`core/src/test/.../WorkflowTest.kt` 覆盖解码、大纲、端口。界面测试至少一条：列表空状态、打开详情。能在模拟器上跑的流程写在实现阶段的 e2e 里，不在本文展开步骤。

## 终端

`host/src/tui/` 没有画布。能力对齐手机的大纲编辑和运行页，键位在实现时先读 `host/src/tui/app/input.rs`，避开已占用的键。帮助页（`Overlay::Help`）里要有这一项。

| 文件 | 职责 |
|---|---|
| `host/src/tui/manage/workflows.rs` | 列表、详情、大纲编辑、运行 |
| `host/src/tui/view/workflows.rs` | 绘制 |

`Overlay` 加 `Workflows(WorkflowSheet)`。花名册上的 `n` 是新建 bot，`w` 是分区，`R` 是例程（`host/src/tui/app/input.rs`）。打开工作流要用一个还没占用的键，并写进帮助页。键只在这张 sheet 打开之后生效，所以 sheet 里面的 `n` 不会跑去新建 bot。

sheet 里：列表 `j` / `k` 移动，Enter 进详情，`n` 新建，`r` 运行，`e` 编辑，`d` 删除前要确认（现有的 `Overlay::Confirm`），`p` 暂停或恢复。编辑时空格切换字段，大纲模式下 `[` `]` 上下移动当前步骤，`a` 添加，`x` 删除步骤。`Fields` 和 `Script` 模式不响应 `[` `]` `a` `x`。`Script` 模式脚本只读，触发器仍可改，状态行用和手机相同的那句英文。审批在运行视图里 `y` 同意、`u` 拒绝（拒绝前确认）。不用 `n` 拒绝，那是新建。

文案走 `host/src/tui/l10n.rs`。不要在 TUI 里再实现一套 cron 或图校验，调用 `workflows::graph::validate` 和 `workflows::outline`。

## 文案

英文句子就是键，和现有的 `L10n.tr("Bots")` 一样。加在 `apps/ios/Kit/Sources/CodyncKit/Resources/Languages/` 的 `en`、`zh-Hans`、`zh-Hant`。桌面 `useTr()` 和 Android `tr()` 读同一批键。键的清单以界面上出现的句子为准，至少包括：

`Workflows`、`New workflow`、`No workflows yet`、`Run now`、`Edit`、`Stop`、`Delete workflow`、`Run {number}`、`Latest results`、`Show older runs`、`Start with a trigger`、`Add trigger`、`Pin`、`Unpin`、`Needs you`、`Script`、`This workflow was edited somewhere else`、`This workflow was arranged on a computer. You can edit each step's settings.`、`This workflow is a script. You can change its trigger. The steps appear when it runs.`、`This workflow decides its steps when it runs`、`This workflow already has a condition`、`Update Codync on this computer to edit this workflow.`、`Choose a bot`、`A run is already in progress`、`This step repeated too many times`、`Approve this step?`、`Version {revision} · Saved {time}`、`Add a trigger first`、`This prompt doesn't feed a step`、`This workflow was deleted`、`This script is too long`、`This script couldn't be read`、`This script uses "{name}", which isn't available`、`The workflow meta must be a literal`、`This step's schema isn't supported`、`The step didn't match the expected shape`、`The step didn't include a prompt`、`This run called for too many steps`、`Too many steps at once`、`Too many stages`、`This parallel didn't get a list of steps`、`The pipeline didn't get a list`、`This workflow's script didn't finish`、`This workflow is a script. Edit the script to change its steps.`。

带插值的句子整句做键，参数用现有的 `L10n` 替换方式，不要拼接翻译过的碎片。

步骤在列表上的标题：触发器用 `Manual`、`Schedule` 或 `Webhook`；提示词用文本的第一行，空则 `Prompt`；智能体用 bot 名，没有则 `Agent`；条件用 `Condition`；审批用问题的第一行，空则 `Approval`；输出用 `Notification` 或那个 bot 的名字。

实现时同步写用户手册 `web/content/docs/workflows.mdx`，并在 `web/app/docs/pages.ts` 登记。只写页面文件的话，`npm run check:docs` 过不了。手册只写用户能做的事，不写表和命令。Android 还没公开，手册里按 `docs/guides/user-docs.md` 先不写 Android。

手机应用的 `apps/ios/WhatsNew.md` 和 `apps/android/WhatsNew.md` 在版本节里各写一段用户能看出来的变化，繁体中文和英文。桌面没有对应的 What's New 文件，不另写。

分析屏幕名加在 iOS `AnalyticsEvent.Screen`、桌面和 Android 已有的屏幕枚举里：`workflows`、`workflow`、`workflowRun`、`workflowEditor`。打开这些屏幕时沿用 `screenViewed`。不要把图或运行输出传给分析。

## 测试

夹具一份，四端各读各的。放在 `host/tests/fixtures/workflow-graph.json`：一张合法图（触发器 → 两个助手 → 输出）、一张条件分支、一张带环、一张手机大纲表达不了的图（两个条件）、一条批注边。host 测试直接读文件。桌面测试把同一份 JSON import 进来。iOS 和 Android 测试里嵌同一段，注释写明来源路径，改夹具时一起改。

host：

- `graph.rs` 单元测试：端口、分支、重复边、上限、字段、cron。
- `outline.rs`：上面五种夹具的 `edit_mode` 和往返（大纲图 `graph_from_outline` 再 `outline_of` 得到同一列）。
- `walk.rs`：两路输入拼成 `Input 1` / `Input 2`；条件 no 边把另一支标 `skipped`；环在第 21 次失败；上游未完成时不跑下游。
- `store.rs`：`baseRevision` 冲突不写；版本 5 分钟内不新增 `auto`；删除后子表没有残留。暂停不增加 `revision`。
- `script.rs`：夹具 `host/tests/fixtures/workflow-script.js`。生成器文法的图能往返，触发器 id 不变。`parallel` 的失败不会变成空结果。第 101 次 `agent()` 失败。`Date` 和 `workflow()` 在保存时被拒绝。重放时已完成的 `output` 不再推送。运行开始之后改脚本，这一次的快照不变。`saveWorkflowGraph` 不能往一张脚本工作流里塞进助手节点。schema 补问不新占 `call-N`。
- `host/tests/workflows_e2e.rs`：用现有的脚本 ACP fixture（`host/tests/fixtures/routine_agent.py` 那一类）跑通助手步骤，断言聊天里有 `workflow-step-*` 通知、步骤 `done`、输出是最终文本。再覆盖停止、审批拒绝、定时跳过忙碌、webhook 重复投递、重启后审批还在。另加一条动态脚本：`parallel` 两个 bot，运行页上先出现阶段再出现两步。

桌面：`node --test` 跑 `workflow.test.ts`、`camera.test.ts`、`edges.test.ts`。相机测试用固定视口断言适应后的 scale。边测试断言路径字符串包含两端点。

iOS：`apps/ios/Kit/Tests/CodyncKitTests/WorkflowTests.swift`，Swift Testing，解码和大纲。

Android：`WorkflowTest.kt`。

终端：`workflows.rs` 里测按键改变大纲后的图，不测绘制像素。

端到端按 `docs/guides/clean-code.md` 的 after-change：Mac 上桌面用 `npm run dev` 走一遍新建、连线、运行、看历史；iOS 模拟器走列表、详情、运行；Android 模拟器走同一条。画布的连线和运行各做一次，不只截一张图。

## 实现顺序

在单独的 worktree 里做（`docs/guides/clean-code.md` 的 after-change，以及 `docs/guides/dev-app.md` 的 sandbox）。不要在共享的 `dev` 工作区里改这些文件。

顺序是为了每一步都能测，不是为了分次合并。合并进 `dev` 时四端和 host 在同一批提交里。

1. schema 7、`model.rs`、`graph.rs`、`outline.rs` 和它们的测试。
2. `store.rs` 的创建、保存、冲突、版本、删除。
3. `walk.rs` 和触发器、提示词、条件、输出的同步执行。先不接 bot。
4. `Cmd::WorkflowStep` 和 e2e。
5. 定时、webhook、推送载荷、`keeps_awake`。
6. `api/workflows.rs`、hello 字段、`workflow` 事件。三端解析事件时只忽略或刷新，此时可以还没有界面。
7. 桌面纯函数和画布。
8. iPhone 和 Android 的列表、详情、编辑、运行、固定、深链。
9. 终端。
10. 文案、What's New、用户手册（`web/content/docs/workflows.mdx` 和 `web/app/docs/pages.ts` 的条目）、`docs/architecture/file-structure.md` 加一行、新写 `docs/features/workflows.md` 作为已实现后的行为说明（本文留在 `docs/design/`，开头改成指向功能文档）。手册要同时写画布和脚本：脚本由 host 执行，bot 在两次运行之间改它。`host/tests/fixtures/wire-shape.json` 要重录：`wire_shape` 把新的 hello 字段算成 added，不重录测试不会过。加字段不是 `minApp` 的提升。

「程式与画布」和上面的画布同一次做完。`script.rs`、`saveWorkflowScript`、四个 bot 工具、桌面脚本面板、手机和终端的 `Script` 模式，不单独拆出去。这些还没落地时，客户端不算做完，功能文档也不要写成脚本已经能跑。

发出去的那一次版本是 minor。当前桌面 `package.json` 是 `2.33.6`；若合并时 `MARKETING_VERSION` 仍等于最新的 `v*` 标签，就把它和 `host/Cargo.toml`、`apps/desktop/package.json` 一起升到下一档 minor，按 `AGENTS.md` 的版本步骤来。已经超前于标签就不要再动。不提高 `MIN_APP`。`host/crates/memory` 和 `host/crates/network` 不改，它们的版本不动。

公开 webhook 复用现有的 `/v1/hooks/{computerId}/{hookId}`，不改 `cloud/` 的路由。合并列表的行为用 host 的中继测试盖住。不因此去部署 cloud。

## 文件清单

新增：

```text
host/src/workflows/mod.rs
host/src/workflows/model.rs
host/src/workflows/limits.rs
host/src/workflows/graph.rs
host/src/workflows/outline.rs
host/src/workflows/store.rs
host/src/workflows/walk.rs
host/src/workflows/run.rs
host/src/workflows/schedule.rs
host/src/workflows/hooks.rs
host/src/workflows/agent.rs
host/src/workflows/script.rs
host/src/workflows/tools.rs
host/src/workflows/service.rs
host/src/api/workflows.rs
host/src/tui/manage/workflows.rs
host/src/tui/view/workflows.rs
host/tests/workflows_e2e.rs
host/tests/fixtures/workflow-graph.json
host/tests/fixtures/workflow-script.js
apps/desktop/src/renderer/views/workflows/   （上一节的文件）
apps/ios/Kit/Sources/CodyncKit/Models/Workflow.swift
apps/ios/Kit/Sources/CodyncUI/Workflows/
apps/ios/Kit/Tests/CodyncKitTests/WorkflowTests.swift
apps/android/core/src/main/kotlin/com/codync/android/core/Workflow.kt
apps/android/core/src/test/kotlin/com/codync/android/core/WorkflowTest.kt
apps/android/app/src/main/kotlin/com/codync/android/WorkflowsView.kt
apps/android/app/src/main/kotlin/com/codync/android/WorkflowDetailView.kt
apps/android/app/src/main/kotlin/com/codync/android/WorkflowRunView.kt
apps/android/app/src/main/kotlin/com/codync/android/WorkflowEditorView.kt
apps/android/app/src/main/kotlin/com/codync/android/WorkflowStepSheet.kt
apps/android/app/src/main/kotlin/com/codync/android/WorkflowOutline.kt
apps/android/app/src/main/kotlin/com/codync/android/AppStoreWorkflows.kt
web/content/docs/workflows.mdx
```

要改的现有文件：

```text
host/src/store/schema.rs                          SCHEMA = 7，migrate_7
host/src/hub.rs                                   workflows 字段，update_keep_awake
host/src/agent/bot/cmd.rs                         Cmd 和 Queued
host/src/agent/bot/queue.rs                       队列臂
host/src/api/mod.rs                               route
host/src/api/http.rs                              /hooks/workflows/{hookId}
host/src/api/events.rs                            hello.workflows，直播 workflow 事件
host/src/api/host.rs                              hello 方法
host/src/store/bots.rs                            删除 bot 时清空节点上的 botId
host/src/remote/push.rs                           notify 增加 WorkflowLink；节流表分开
host/src/remote/relay.rs                          publish_hooks 合并两份；hook_item 分发
host/Cargo.toml                                   增加 regex、boa_engine
host/tests/fixtures/wire-shape.json               hello.workflows
host/tests/e2e/wire_shape.rs                      重录，不改断言逻辑
host/src/tui/app/ui.rs                            Overlay::Workflows
host/src/tui/app/input.rs                         打开工作流的键
host/src/tui/l10n.rs
apps/desktop/src/renderer/client/host-client.ts  解析 workflow 事件和 hello 字段
apps/desktop/src/renderer/store/bot-sync.ts      apply 时刷新，不 bump rev
apps/desktop/src/renderer/views/ComputerRosterSection.tsx
apps/desktop/src/renderer/views/PageRail.tsx
apps/desktop/src/renderer/views/ChatWindow.tsx
apps/desktop/src/renderer/lib/prefs.ts            固定的工作流 id
apps/ios/Kit/Sources/CodyncKit/Client/HostClient.swift
apps/ios/Kit/Sources/CodyncUI/Store/BotStore+Sync.swift
apps/ios/Views/ComputerSection.swift
apps/ios/Views/BotListView.swift
apps/ios/Views/RootView.swift                     通知深链，先清掉打开的聊天
apps/android/core/.../Conversation.kt             MirrorEvent
apps/android/app/.../BotListView.kt
apps/android/app/.../ComputersView.kt
apps/android/app/.../MainActivity.kt              userInfo 里的 workflowId
apps/android/app/.../AppState.kt                  工作流路由
apps/android/app/.../MainTabs.kt                  NavStack 多一项
apps/ios/Kit/Sources/CodyncKit/Resources/Languages/
apps/ios/WhatsNew.md
apps/android/WhatsNew.md
docs/architecture/file-structure.md
docs/features/workflows.md                        实现完成后新写，不是改现有文件
web/app/docs/pages.ts                             用户手册条目
```

不改 `host/src/routines/` 的存储和 `Cmd::Routine`。日程预览和 webhook 的 `authorized` 只调用，不复制一份。
