# 本地修改说明（EasyMD）

本目录是 [egui_commonmark](https://github.com/lampsitter/egui_commonmark) 0.25.0 的**本地副本**，
仅用于承载 EasyMD 需要的一处渲染修改。上游以 MIT / Apache-2.0 双许可发布（见 `LICENSE-*`）。

## 修改内容

**文件**：`src/parsers/pulldown.rs`
**函数**：`CommonMarkViewerInternal::table`（以及新增的辅助函数 `table_cell`）

**改前**：每个表格单元格用「不换行的水平布局」（`ui.horizontal`）渲染，
`egui::Grid` 也不限制列宽。因此列数一多、单元格文字一长，整张表就会横向超出阅读栏宽，
超出的部分被直接裁掉，用户既看不到也无法滚动。

**改后**：**按内容量分配列宽**（`column_widths` + `cell_natural_width`）。

1. 先量每列「不换行的自然宽度」：取该列所有单元格里最宽的，正文文字用
   `TextStyle::Body`、行内代码用 `TextStyle::Monospace` 的字体量（与渲染用的字体一致），
   再留 2px 容差——折行布局按浮点比较「放不放得下」，宽度正好等于文字宽度时
   会因为亚像素舍入把最后一个字挤到下一行。
2. 总宽装得下就按自然宽度（**一格都不换行**，表格宽度贴合内容，和浏览器
   `width: max-content` 的行为一致）；装不下才按内容比例压缩到刚好铺满栏宽
   （此时才会换行，且右边不会留下大片空白）。
3. **自己排版，不用 `egui::Grid`**（`Grid` 的列宽/行高语义与自定义列宽互相打架：
   它会从列宽里再扣一次 `item_spacing`、按错误的单元格尺寸算行高，结果是行与行重叠）。
   现在每行是 `ui.horizontal_top`，列间距由它的 `item_spacing.x` 提供；行高由"最高单元格"
   自动决定；斑马纹用 `ui.painter().add(Shape::Noop)` 占位、渲染完再 `set` 成矩形，
   这样背景画在文字下面。
4. **表格必须自己开一个竖向布局**（`ui.vertical`）：文档正文是在"自动换行的水平布局"
   （内联流）里排的，不套 `vertical` 的话各"行"会被当成内联元素排到同一条线上。
5. 单元格：`ui.allocate_ui_with_layout(vec2(列宽, 0.0), top_down)` +
   **`ui.set_min_width(列宽)`** + `ui.horizontal_wrapped`。
   `set_min_width` 是关键——egui 里文本标签**不会推进布局游标**（只有 `allocate_*` 会），
   不把列宽显式写进单元格的 `min_rect`，这一列就会被当成"只有文字那么宽"：
   右边几列全部左移挤成一团，行高也会算成 0 而让行与行重叠。
6. 渲染与度量都要跳过表格结构事件（`is_table_boundary`）：`parse_table` 会把
   「上一格的 `End(TableCell)`」放进下一格的事件列表，而上游在
   `TagEnd::TableCell` 处会 `ui.label("  ")`（原本是给水平布局做列间距的）。
   自己排版后这个分隔符只会吃掉 8px 宽度。

> 走过的弯路（两次）：
> 1. 「可用宽度 ÷ 列数」给每列同一个上限 → 长列被压到平均值就换行，总宽却远小于栏宽，
>    右边一片空白、格子里的字却在换行。列宽必须按**每列自己的内容量**分配。
> 2. 用 `Grid` + 每格固定宽度 → 间距被扣两次导致"放得下的字被迫换行"，
>    且行高按错误的单元格尺寸计算，行与行叠在一起。表格布局自己写反而更短更可控。

## 升级上游版本时的做法

1. 用新版本覆盖本目录（保留 `PATCHES.md` 与两个 `LICENSE-*`）。
2. 在新版 `src/parsers/pulldown.rs` 里找到 `fn table`，把「计算列宽 → 竖向排列各行 → 固定宽度渲染
   单元格」这套逻辑套回去：新增 `column_widths` / `cell_natural_width` / `measure_text` /
   `is_table_boundary` 四个辅助函数与改写后的 `table_cell`，
   `Grid` 前把 `item_spacing.x` 置零，两处渲染闭包开头跳过 `is_table_boundary`。
3. 运行 `cargo test tables_size_columns_by_content`（在 `src/reader.rs` 里，会真实渲染表格并量
   宽度与行数），再用一份列多的 Markdown 表格在阅读模式里肉眼确认：内容放得下时不换行、
   超宽内容铺满栏宽、表格完整显示在栏宽内、没有横向滚动条。

> 备注：EasyMD 刻意**不启用** `better_syntax_highlighting`（syntect），
> 代码高亮使用 `egui_extras` 内置的轻量实现，因此本副本也不会链接 syntect。

---

## 改动二：渲染锚点（`src/parsers/pulldown.rs` + `src/lib.rs`）

EasyMD 的「编辑模式左右两栏联动」需要「源码字节偏移 → 渲染纵坐标」的映射。

- `CommonMarkViewer::block_positions(&RefCell<Vec<(usize, f32, f32)>>)` 注册收集器。
- 收集的锚点是**实际绘制出来的元素矩形**：在 `event_text` 里用 `ui.label(...)` 返回的
  `Response::rect`（减去内容原点）记录行顶/行底，而不是去猜布局游标。
- 之所以不能推断：egui 的自动换行布局里，文本标签是放在"当前行"内部的，
  `ui.next_widget_position()` / `ui.min_rect()` 都不会因为文本而推进纵坐标
  （只有 `allocate_*` 类部件才会），于是所有段落会得到同一个过时的 y，联动就会错位。
- 偏移按单调不减整理，同一偏移换了行则追加一项，便于上层二分查表。
- 上层用法：`range_for_offset(offset)` 取该偏移所在行的 (行顶, 行底)；
  高亮块的底边取「块范围内所有锚点底边的最大值」，比用块的末尾偏移查表更稳
  （末尾偏移可能落到下一块的行上，尤其是标题行尾会被注入锚点属性拉长）。

## 升级上游版本时的做法（改动二）

1. 在 `src/parsers/pulldown.rs` 的 `CommonMarkViewerInternal` 上加两个字段：
   `diag_content_top: f32` 与 `placements: Vec<(usize, f32, f32)>`；
   在 `show()` 开头记录 `content_top` 并清空 `placements`。
2. `event_text` 增加一个 `src_span: Range<usize>` 参数，在 `ui.label(...)` 之后把
   `(span.start, rect.top() - content_top, rect.bottom() - content_top)` 推进 `placements`；
   三个调用点（Text / Code / InlineHtml）都要带上 `src_span.clone()`。
3. 在 `show()` 的事件循环里 `process_event(...)` 之后，把 `placements` 排空写入 `block_positions`，
   保持偏移单调、同一偏移换行则追加。
4. `src/lib.rs` 的 `CommonMarkViewer` 增加 `block_positions` 字段与同名 builder 方法，
   并把它透传给内部 `show`（内部签名多一个参数，`show_scrollable` 传 `None`）。

---

## 改动三：正文文字块点击上报（`src/parsers/pulldown.rs` + `src/lib.rs`）

EasyMD 的「编辑模式下点预览正文 → 编辑器光标跳到对应块」需要知道**点了哪一块**。

- `CommonMarkViewer::text_clicks(&RefCell<Vec<usize>>)` 注册收集器；**只有注册了收集器**
  文字块才会感应点击，默认仍是纯 hover，不影响其它交互。
- 内部实现：`event_text` 里把 `ui.label(...)` 换成
  `ui.add(Label::new(...).sense(Sense::click()))`（仅当 `text_click_enabled`），
  被点击时把该块的 `src_span.start` 推进 `clicked_text`；`show()` 结束时再灌进收集器。
- 为什么不复用 `block_positions` 的 y 坐标：文字块会**吃掉点击**（egui 的命中测试里，
  上层 label 让下层控件的 `hovered()` 为 false，而 `clicked()` 需要 hover），
  所以「在正文区叠一个大矩形去接点击」在文字上永远收不到事件，只能在行间空白生效。
  由文字块自己上报偏移既精确又不依赖像素。

## 升级上游版本时的做法（改动三）

1. `CommonMarkViewerInternal` 加两个无生命周期字段：`text_click_enabled: bool`、
   `clicked_text: Vec<usize>`（构造时初始化）。
2. 内部 `show()` 增加 `text_clicks: Option<&RefCell<Vec<usize>>>` 参数：
   开头设 `text_click_enabled = text_clicks.is_some()` 并清空 `clicked_text`，
   结尾把 `clicked_text` 追加进收集器（`show_scrollable` 的内部调用传 `None`）。
3. `event_text` 按开关选择 `ui.label` 或 `Label::new(..).sense(Sense::click())`，
   并在 `resp.clicked()` 时记录 `src_span.start`。
4. `src/lib.rs` 的 `CommonMarkViewer` 加 `text_clicks` 字段与 builder，并透传给内部 `show`。

---

## 改动四：修复「引用块内含列表」触发的 panic（`src/parsers/pulldown.rs` + `src/lib.rs`）

**症状**：打开某些 Markdown 后整个程序 abort：

```
thread 'main' panicked at vendor/egui_commonmark/src/lib.rs:402:13:
internal error: entered unreachable code
```

**触发条件**：文档中**先出现「引用块里含列表」**（例如 `> - 甲`），并且其后还有足够多的内容。
实测一个 16 行的最小块单独打开不崩，前面接上另一篇文档的前 177 个块也不崩，
两者**合起来**必崩——所以是跨块的解析状态被污染，而不是某个孤立语法。

**根因**（上游缺陷，三处叠加）：

1. `blockquote()` 渲染引用块内的子事件时用的是 **`self.event()`**，而不是 `self.process_event()`。
   而消费 `is_list_item` 标志的 `item_list_wrapping()` **只在 `process_event()` 里调用** ——
   于是引用块内 `Start(Item)` 置上的 `is_list_item` **永远不会被消费**，泄漏到外层。
2. 外层紧接着为「引用块的 `Start` 事件」执行 `process_event()`，其中的 `item_list_wrapping()`
   带着这个残留标志调用 `delayed_events_list_item()`，把引用块**之后**的外层事件吞进一个子列表里渲染
   （该函数遇到 `Start(List)` 会**提前返回并把该事件包含进结果**），事件流由此错位。
3. 错位后，某个 `Start(Item)` 到达时 `List::items` 已经是**空栈**，撞上上游的
   `unreachable!()` → `abort()`。

**改法**（防御 + 堵源头，两层）：

- `parsers/pulldown.rs` / `blockquote()`：进出引用块时**保存/恢复** `self.is_list_item`，堵住泄漏。
  引用块**内部**的渲染行为完全不变（列表项在引用块里本来就不走 `horizontal_wrapped`）。
- `lib.rs` / `List::start_item()`：去掉崩溃路径 —— 空栈时补一个
  `ListLevel { current_number: None }`，按最外层项目符号画。
  这一层兜底同时覆盖另一条路径：`show()` 的**视口切片**
  （`skip(first_event_index)`，切片起点可能落在列表中间，而被跳过的 `Start(List)` 不会补回来）。

## 升级上游版本时的做法（改动四）

1. `src/parsers/pulldown.rs` 的 `fn blockquote`：函数体开头存 `let saved_is_list_item = self.is_list_item;`，
   结尾（`self.is_blockquote = false;` 之后）恢复 `self.is_list_item = saved_is_list_item;`。
   若新版已把引用块也改走 `process_event()`，则本改动不再需要（届时删掉即可）。
2. `src/lib.rs` 的 `List::start_item`：把 `else { unreachable!() }` 之前补上
   `if self.items.is_empty() { self.items.push(ListLevel { current_number: None }); }`。
3. 回归：`cargo test`（本副本当前 51 项全过），再打开任一含「引用块 + 列表」的文档确认真不崩。
