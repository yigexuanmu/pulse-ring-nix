# 歌词识别模块技术文档

本模块是从 [noctalia-dev/community-plugins](https://github.com/noctalia-dev/community-plugins/tree/main/lyrics) 的 `lyrics` 插件中提取的**歌词识别与提取部分**（仅数据获取与解析，不含任何动画/渲染/显示逻辑），为纯 Python 标准库实现，无第三方依赖，可直接独立使用或被宿主程序（如 pulse-ring）以子进程方式调用。

## 文件清单

| 文件 | 作用 |
| --- | --- |
| `lyric_sources.py` | 核心：多歌词源适配器 + 各种歌词格式解析 + 命令行入口 |
| `krc_decode.py` | 网易云 KRC 动态歌词（klyric 字段）解码器 |
| `lrclib_lyric.py` | LRCLIB 精简独立客户端（早期/单文件版本） |
| `test_lyric_sources.py` | `lyric_sources.py` 的单元测试 |

> `lyric_sources.py` 已内置 LRCLIB、网易云、QQ 音乐、酷狗、SPlayer、Spotify、Apple Music、Musixmatch 等源，`lrclib_lyric.py` 是其功能子集，二选一即可。

## 统一歌词数据模型

所有源最终被归一化为「行 + 独立字段」的结构（时间单位为毫秒）：

```json
{
  "time": 1200,           // 该行开始时间(ms)，-1 表示无时间轴(纯文本)
  "duration": 1800,       // 该行持续时长(ms)，可自动推算
  "text": "Original lyric",        // 原文
  "translation": "译文",            // 翻译（可空）
  "romanization": "pinyin/romaji", // 罗马音（可空）
  "chars": [1200, 1500, 1800]      // 逐字时间戳（供逐字定位，数据层不做渲染）
}
```

顶层响应结构：

```json
{
  "type": "lyrics" | "none",
  "source": "lrclib",
  "lines": [ { ...统一行模型... } ],
  "diag": ["lrclib: match"],
  "cover": "https://.../art.jpg"   // 可选，来自源或 iTunes 兜底
}
```

- `type == "none"` 表示该源未匹配到歌词，`diag` 描述失败原因。
- 行模型中的时间戳均为**毫秒**；MPRIS 侧传入的曲目时长为微秒时由 `duration_ms()` 自动换算。

## 调用方式（CLI）

`lyric_sources.py` 通过文件路径传递请求，请求文件读完即删（避免凭据落盘）：

```sh
echo '{
  "source": "lrclib",
  "track": {"title": "...", "artist": "...", "album": "...", "duration": 269000000},
  "credentials": {},
  "options": {"lyrics_candidate_id": "10"}
}' > /tmp/req.json
python3 lyric_sources.py /tmp/req.json
# -> 输出 JSON（stdout），ensure_ascii=False
```

请求字段：

- `source`: 源 ID（见下节）。
- `track`: 必填 `title`；可选 `artist` / `album` / `duration`（微秒或毫秒均可）。
- `credentials`: 各源所需令牌/URL（如 `spotify_sp_dc`、`musixmatch_token`、`splayer_api_url`）。
- `options`: 例如 `lyrics_candidate_id`（LRCLIB 多结果时指定某候选）、`translation_language`。

## 支持的歌词源

| 源 ID | 说明 | 是否需要凭据 |
| --- | --- | --- |
| `lrclib` | LRCLIB 公开搜索，同步歌词自动排序 + 手动选择候选 | 否 |
| `netease` / `netease_public` | 网易云搜索 + 歌词/翻译/罗马音接口 | 否（可能有地区限制） |
| `qqmusic` / `qq` | QQ 音乐搜索 + 歌词(base64) + 翻译 + 罗马音 | 否 |
| `kugou` | 酷狗搜索 + 歌词下载(lrc) | 否 |
| `splayer` | 从本地运行的 SPlayer 拉取当前歌词（含逐字/背景/对唱标记） | 否，但需 SPlayer 运行，默认 `http://127.0.0.1:25884` |
| `qishui` | 用户自定义 HTTP 端点，模板支持 `{title}` `{artist}` `{album}` | 可选 Bearer token |
| `spotify` | Spotify 搜索 + color-lyrics 逐字歌词 | `spotify_access_token` 或 `spotify_sp_dc` |
| `apple_music` | Apple Music 目录 + 歌词 | `apple_developer_token` 必填，`apple_user_token` 可选 |
| `musixmatch` | Musixmatch 字幕接口 | `musixmatch_token` |
| `mpris` / `custom` / `external` | 由宿主（Noctalia）直接提供的歌词，不在本适配器内 | — |

自动模式按配置顺序尝试各源，失败时**不打印凭据**，直接切换到下一个源。

## 歌词格式解析

所有解析函数均以毫秒时间戳产出统一行模型：

- **LRC** `parse_lrc(text)`：`[mm:ss.xx]` 标签；支持 `[offset:]` 整体偏移、元信息头（`[ar:]` 等）跳过、歌词前两行的信用行过滤；支持增强 LRC `<mm:ss.xx>word` 逐字时间与 KRC 内嵌标签。
- **KRC（网易云逐字）** `parse_lrc` 内联解析 + `krc_decode.py`：`krc1` 魔数 + zlib 流解码；支持 `(offset,duration,0)word` 前缀与 `word<offset,duration>` 后缀两种逐字变体，输出 `chars` 逐字时间。
- **QRC（QQ 逐字）**：`QRC_SUFFIX_WORD` 与 `qrc_content()` 处理 `<LyricContent>` 或 XML 包装。
- **TTML** `parse_ttml(text)`：XML 解析 `<p begin end>`，自动把带 `translation` / `roman` role 的 `<p>` 合并进主行。
- **JSON 行数组** `parse_json_lines`：兼容 `time/start/startTimeMs`、`end/endTime`、`text/lyric/words`、`translation`、`romanization`、`chars/charTimes` 等多种字段命名；自动探测 TTML/JSON/LRC。
- **纯文本** `parse_plain`：无时间轴的普通多行文本。
- **SPlayer 结构** `splayer_transmitted_lines`：解析 `yrcData`/`lrcData` 字段，保留 `words` 逐词时间、`isBG` 背景行、`isDuet` 对唱、译文与罗马音，并对拉伸单字做 `duration_inferred` 标记。

### 时间推断（finalize）

- 行无 `duration` 时，取下一行时间差作为时长（最后一行用曲目总时长）。
- `time < 0` 的行排序靠后，视为无时间轴行。
- `merge_timed(primary, secondary, field, tolerance=500)` 按时间就近把翻译/罗马音合并到主行，超差则按序号回退到无时间轴行。

## 匹配算法

- `best_match(items, track, ...)`：标题 6/3 分、歌手 4/2 分、专辑 2/1 分，得分 ≥3 才接受，取最高分。
- `lrclib_candidates(items, track)`：LRCLIB 专用排序——先身份匹配（标题/歌手/专辑），再按时长分桶（差 ≤2s 最优），桶内优先同步歌词，最后按原始顺序；去重（按 `id`）、过滤缺 `id` 或歌词为空的项。
- `first_value(data, names)`：递归搜索 JSON 中指定键名的首个非空值（兼容各家 API 命名差异）。
- `first_cover(...)` / `itunes_cover(track)`：封面提取，支持 `//` 协议补全、尺寸模板替换（如 `{w}`→`400`）、iTunes 兜底（把 `100x100bb` 换成 `400x400bb`）。

## 安全与健壮性

- 仅用标准库 `urllib`，所有网络请求带超时（默认 15s，SPlayer 探测 1s 重试 3 次）。
- 请求文件读取后立即删除；凭据只在内存中经 `credentials` 传递，**从不打印**。
- 所有 API 异常被 `main()` 的异常树捕获并转为 `empty()`（`type:"none"` + diag），不崩溃。
- 输出 `ensure_ascii=False`，中文歌词不被转义。

## 测试

```sh
python3 -m py_compile lyric_sources.py krc_decode.py lrclib_lyric.py
python3 -m unittest test_lyric_sources.py
```

覆盖点：LRCLIB 候选排序（优先同步/按时长分桶/去重/按 id 指定）、SPlayer 时序层与标记保留、无效 API 的有界重试等。

## 接入 pulse-ring 的建议

pulse-ring 目前没有任何文本显示 widget（见仓库分析：`5a3d8c6` 移除了 text widget）。本模块仅提供歌词数据（获取 + 解析 + 统一行模型），渲染/动画部分由宿主另行实现。

接入方式：定时把当前 MPRIS `xesam:title/artist` 写入请求文件并调用 `python3 lyric_sources.py`，即可拿到统一行模型（含 `chars` 逐字时间戳），供宿主后续自行渲染。
