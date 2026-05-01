# User
最后再核查制定一遍计划：先align我与你的想法，根据 src-tauri/src  src 情况，store插件的用法见https://v2.tauri.app/plugin/store/，若查找网页失败多次就不要查找而在生成结果中显示查找什么失败，补全我思考的架构并寻找潜在的缺陷或疏漏点。
产品的需求起源于我作为ADHD+高敏感人士，一方面容易沉迷（尤其是工作）而久坐危害健康甚至直接影响当下的效率而不自知，但另一方面常规timer的通知提醒、声音提醒、任务栏闪光提醒都太「刺激」了，会惊吓到我或让我感到被打断的愤怒，所以需要一个更温和和渐进的提醒方式。
典型使用场景：编写某个程序，例如本项目，同时开多个软件一边阅读一边写代码一边用agent，可能同时还在纸上写写画画；此时为了防止自己沉迷这项工作，配置好开启饱和度和色温通道，设置此次工作时长（根据我自己过往的经验来）后，关闭配置窗口，用快捷键启动该timer；有时候会期望在tray erea能看到一个圆形进度条看还剩多少时间；当到设定时间的最后5%（我自己如此配置）时，屏幕开始渐进地降低饱和度和色温，直至到设定时长达到峰值；**当我察觉到（意思是工作时间不取决于硬性的钟表提醒，而取决于我的工作状态和当下的感官敏感度）屏幕上变化时，起身休息**；回到电脑桌前按一个快捷键后屏幕恢复到正常态；允许设定若干个（例如三个）快捷键对应不同时长，按下快捷键后直接开启对应时长的一次计时；tray也要有快捷键的那些功能以应对键盘不方便使用的场景；出于可能要视觉创作的场景，将来会加入声音或暗角等其他感官通道提醒；在计时过程中，可能有的用户提前自己起来休息，而且尽管我们期望用户起来休息时完全不用管我们这个程序但他仍然习惯让感官通道直接达到峰值，所以要允许一个快捷键让应用从当前状态快速平滑地攀升至峰值（settling）；同时有可能用户正在开线上会议而忘记关某次计时，当他察觉到后要有一个快捷键允许迅速从当前状态平滑回落至正常态；当用户设置时，允许用户配置各个感官通道开闭与否，峰值为多少，时长为多少，在后百分之几开始感官变化，并且允许用户开启预览模式后直接在配置界面拖动进度条来模拟时间进度而浏览效果；用户不应当关心该程序是否运行——后台自启动，设计上不鼓励用户完全关闭此程序，取而代之的是若用户不希望被提醒直接不开启一段计时就行。
状态机状态设立如前所述，在当前代码基础上添加一个Preview状态。各感官通道根据用户配置完成各自的数值计算。渲染器根据感官通道计算的结果渲染视觉和未来的听觉效果。使用官方的tauri-plugin-store插件来持久化配置。
一个完整的应用生命周期：先读取配置生成各个子组件——TimerStateMachine、SendoryChannels、Renderer、mpsc channel (Renderer要根据通道开闭情况优先加载好自身，加载逻辑参考后面loop的描述），随后将除了tx的组件全部move给engine而实例化出engine，engine生命周期应当和整个app生命周期同步；前端不弹出界面，除非是按下快捷键或tray按钮；随后开启engine的loop，此loop跑在一个独立的同步线程中，并且独占engine所有权；loop中首先接收来自各方的信息，随后把它根据去向组装成一个struct，之后计算时间，而后状态机消费各个cmd并根据cmd和时间推进状态，然后通道系统接收cmd和状态更新自身状态并得到感官数值，若有通道配置更改则需要持久化、因为官方插件自带防抖动设计（允许设置一个防抖动保存时间）而且一提交（set，insert）就返回值、无需等待、所以可以在此处持久化、也意味着进入第三阶段——副作用阶段，若有通道关闭、要通知renderer系统检查是否有某个subrenderer对应的所有通道都关闭了、如果有则关闭、如果某个关闭的subrenderer对应的通道开启了则开启之；随后Renderer消费感官数值、分发给不同平台实现不同的subrenderer、并且可能需要运行在主线程或其他线程；应用运行过程中可能会出现问题，所以允许用户直接强制初始化所有renderer，这里采取归零重启的逻辑；若loop过程panic了也应当关闭所有renderer并提示用户，但提示方式不应当太「扎眼」或「刺耳」；当应用退出时，捕获退出信号（没有退出信号的直接杀进程管不了），并异步地通知engine去shutdown，此时主线程交还占用防止死锁，engine收到后先记录好该记录的log，随后走shutdown逻辑，异步通知所有renderer关闭，交回控制权异步调用.save让store存储状态，随后等待以上两个shutdown逻辑跑完或超过合理时间，超过合理时间记录log，随后再次提交一次退出信号，主线程这次捕获后直接退出。
悬而未决的问题。若某个subrenderer运行在其他线程上出了问题（不论是正常运行还是执行关闭逻辑），我的engine无法得知。若运行出现问题而导致那一线程卡住，等到退出时又新开一个线程来关闭可能会出现ub或所有权问题。engine的生命周期如何绑定成与app相同。tauri channel持有问题，前端窗口打开时需要订阅进度，后端循环才会拿到channel，而窗口一旦关闭会不会影响到channel存亡与否，而再次打开窗口要订阅进度时又怎么办。

你生成回复时卡住了，继续生成完毕，上次回复的思维链如下供参考：
I'm getting the full picture now. The Store from Tauri's plugin is an Arc wrapper so it's easily cloneable, set() operations are synchronous with background debounced persistence, and save() is also synchronous. The key insight is that the engine doesn't need to implement its own debounce logic—the store handles that natively with a 2-second window. I just need to call store.set() whenever a command modifies config and let the store manage the rest.

Looking at the actual codebase, most of the old modules like compositor, phase, platform, and session are stale artifacts, while tray.rs is still active and working directly with the AppHandle. The lib.rs file is broken because it's importing modules that either don't exist anymore or have been reorganized elsewhere—compositors was migrated to the renderer, and commands is missing entirely with only old_commands.rs remaining. I need to audit what's actually present and clean up the module declarations.

The user's architectural vision is clear: read config, instantiate components, move everything into the Engine, spawn it on its own thread with independent ownership, then have the main loop receive info, assemble it by destination, run the state machine, persist config changes through the plugin's built-in debounce, notify the renderer if channels toggle, and handle panics by closing renderers gracefully before shutdown.

For shutdown, the engine gets an async signal, main thread yields, engine logs and notifies renderers asynchronously, calls store.save() to block until disk write completes, waits for both with a timeout, logs if it times out, then re-emits the exit signal so main thread can finish.

The open questions are around sub-renderer failures on other threads (potential UB during exit), binding engine lifecycle to app lifecycle, and managing the Tauri channel subscription when the window opens and closes.

Now I'm thinking through the plugin store strategy — skip custom debounce since the plugin handles it, have the engine hold an Arc<Store> for clone-friendly send/sync access, let set() fire-and-forget with debounce in the background, and call save() synchronously at shutdown to block until the disk write finishes. For the store key structure, I could either use a single "config" key with the full AppConfig JSON or split it into multiple keys per logical section like channels.saturation, channels.color_temperature, and timer settings.

For Q1 about sub-renderer health, the WindowsColorTransformer doesn't actually spawn its own thread — it dispatches to the Tauri main thread via run_on_main_thread, and the mpsc channel handles confirmation through RendererEvent responses. I can track outstanding dispatches with a counter that increments on each apply call and decrements on each event; if the counter stays above a threshold for too long, I suspect the main thread is stuck, log a warning, try a reset, and mark the renderer as degraded. At shutdown, if the recv_timeout elapses, I don't spawn a retry thread — I just log and proceed since the main thread is already pumping.

For future sub-renderers that do spawn their own threads, each will carry a JoinHandle and cancellation token, and the engine's shutdown becomes a contract where it signals cancel and joins with a timeout for each one.

For Q2, the engine lifecycle is tied to the app lifecycle — it's spawned in setup, stored via app.manage, blocks on event_rx.recv, and only exits on Shutdown. The exit hook sends Shutdown then joins, and if the engine thread panics, the JoinHandle detects it and logs but continues teardown.

For Q3, Tauri's ipc::Channel is just a function wrapper around an internal IPC route, so when the frontend window closes, the channel becomes invalid but undetectable from the backend — sending returns Ok but the message silently drops.

When the window reopens, the frontend needs to invoke register_progress_channel with a fresh channel object. I'll use EngineEvent::SetProgressChannel and EngineEvent::ClearProgressChannel to manage this — the frontend registers on mount and optionally unregisters on unmount, while the engine just replaces the held channel each time. Vue/React unmount fires when the component dies, but window hide doesn't trigger unmount, so a hidden window keeps the channel valid even if the JS receiver isn't subscribed. The engine starts with None, sends nothing until a channel is registered, then flows data without buffering since the current frame is sufficient.

Now I'm flagging several implementation gaps: lib.rs is broken with missing module references, commands.rs doesn't exist but is referenced, old_commands.rs exists but isn't properly integrated, and Cargo.toml is missing tauri-plugin-store and tauri-plugin-autostart. The frontend visibility logic needs tauri.conf.json configured with windows[0].visible = false so the window stays hidden until a tray click or shortcut triggers it. The three configurable shortcuts for different session durations can leverage the existing StartSession command that already accepts parametric durations.

I need to add a force-reset mechanism as a new EngineEvent that calls try_reset on all sub-renderers. For the panic handling, emitting a Tauri event that the frontend can display as a quiet banner on next launch is more reliable than trying to show UI from a panic hook—I'll flag the crash in the store and let the frontend check it on startup. There's also a typo in TimerStateMachine.current_elapsed_ms and pending channel logic fixes for Update::Unchanged propagation. The Sender type for Renderer needs clarification on what event type it should carry.

For the tray display, I need to wire up state machine updates to refresh the tray title so it shows progress. The loop should use adaptive intervals based on state machine recommendations, keeping the responsive behavior from the old session.rs. On macOS, all channels stay defined but the renderer ignores Saturation, and I'll expose an availability check so the frontend can hide unsupported controls. When a channel toggles off, the smooth transition to neutral is the right behavior—the channel value snaps to neutral and propagates as a Changed update while the sub-renderer unload happens separately, avoiding jarring visual jumps.

There's a bug where the tick immediately sets the value to neutral instead of smoothly transitioning, which I'll flag as a TODO for v1. I should add a Drop impl for EngineHandle to log if it's dropped prematurely and the JoinHandle leaks. The Renderer trait needs its Sender type unified to EngineEvent. The channels definition should consolidate the platform-specific cfg blocks into a single invocation with the availability helper. Finally, SensoryChannel::tick needs to receive FrameEvents instead of FrameFlags, and the engine must manage clearing the just_transited flag between ticks.

Now I'm organizing the findings into a clear structure: current state, architectural alignment with the plugin store, solutions to the open questions, newly discovered issues, and a batched implementation plan.

# Agent
继续生成。我已经确认了 Store 插件用法，并扫了一遍 `src-tauri/src` 的当前状态。先做一次审计再给出对齐后的最终方案。

---

## Part 1 · 当前代码状态审计

### 1.1 实际存在的文件

```
src-tauri/src/
├── lib.rs              ❌ 引用了不存在的模块（compositors, commands）
├── main.rs
├── engine.rs           ⚠️ 仅有 FrameFlags 占位（9 行）
├── channels.rs         ✅ 平台分裂的 define_channels!
├── timer_state_machine.rs ✅ 含 cerrent_elapsed_ms 拼写
├── configs.rs          ⚠️ 几乎空（只有 settling/reverse duration）
├── observability.rs    ⚠️ 走 ManagedObservability，与"engine 独占"原则冲突
├── tray.rs             ✅ 用 app.emit 投递事件（待改为 EngineEvent）
├── utils.rs            ✅ Update<T>
├── events/
│   ├── mod.rs          ✅
│   ├── timer_state_commands.rs    ⚠️ 缺 EnterPreview/ExitPreview/UpdatePreviewProgress
│   ├── channel_commands.rs        ✅
│   └── renderer_events.rs         ✅
├── renderer/
│   ├── mod.rs          ✅ Rendering trait
│   ├── windows.rs      ✅ 已实现 transform_color，缺 try_shutdown/try_reset/switch
│   └── macos.rs        ❌ 仅 `pub struct Renderer;` 空壳
├── compositor.rs       🗑️ 旧
├── platform.rs         🗑️ 旧
├── phase.rs            🗑️ 旧
├── session.rs          🗑️ 旧
└── old_commands.rs     🗑️ 旧（lib.rs 引用的是 `commands::` 不是 `old_commands::`）
```

### 1.2 编译应当已经断了

`lib.rs` 引用：
- `mod compositor; mod compositors; mod platform; mod phase; mod session;` — `compositors` 模块文件不存在
- `commands::preview_frame` / `commands::reset_display` — `commands` 模块根本没在 mod 列表里，文件名实际是 `old_commands.rs`
- `session::ManagedSessionController` — 旧系统

可以手动 `cargo check` 确认，但根据当前 lib.rs 的内容**不可能编译通过**。Batch 0 必须先做。

### 1.3 Cargo.toml 缺失依赖

当前依赖：
```toml
tauri = "2.10.3"
tokio = { version = "1", features = ["time"] }
nalgebra, serde, serde_json, thiserror
windows-sys (windows only)
```

**缺**：
- `tauri-plugin-store` ← 持久化
- `tauri-plugin-global-shortcut` ← 全局快捷键
- `tauri-plugin-autostart` ← 后台自启动（用户需求："后台自启动，设计上不鼓励用户完全关闭"）
- `tokio` 需要补 `sync`、`rt-multi-thread` features 才能用 mpsc 等
- 可选：`tracing` + `tracing-subscriber` 替代当前手撸 observability

---

## Part 2 · 与你的架构对齐 + Store 插件带来的简化

### 2.1 Store 插件核心特性

```rust
let store: Arc<Store<R>> = StoreBuilder::new(app, "config.json")
    .auto_save(Duration::from_secs(2))     // ← 关键：内置 2s debounce
    .default("channels", default_channels) // 默认值
    .build()?;

store.set("key", json_value);              // 同步返回；后台 debounce 落盘
store.save()?;                             // 同步阻塞直到落盘完成
let v: Option<JsonValue> = store.get("key");
```

#### 直接消除你架构里的两个组件

1. **去掉自实现的 `Persistence { last_dirty_ts }`**：Store 自带 debounce
2. **去掉"shutdown 时 spawn save_worker 再 join"的复杂逻辑**：直接 `store.save()` 同步阻塞即可

#### 引入新的考量

- `Store<R>` 是 `Arc` 包裹的；可以 cheap clone，跨线程持有
- `set()` 是 fire-and-forget（同步返回，落盘异步）—— **意味着 dirty 标志在后端代码里完全不需要**，只要每次命令处理时 `store.set(...)` 即可
- `save()` 同步阻塞意味着 shutdown 路径不需要 spawn worker thread

### 2.2 Engine 持有什么

```rust
pub struct Engine<R: Runtime> {
    // 业务状态
    timer:    TimerStateMachine,
    channels: SensoryChannels,
    renderer: Renderer,
    
    // 协作通道
    event_rx: tokio::sync::mpsc::Receiver<EngineEvent>,
    event_tx: tokio::sync::mpsc::Sender<EngineEvent>,  // 自己留一份用于 register sink 等
    
    // Tauri / 持久化
    app:      AppHandle<R>,
    store:    Arc<Store<R>>,
    
    // 出站 sink
    progress_to_frontend: Option<tauri::ipc::Channel<ProgressPayload>>,
    last_tray_update:     Instant,
    
    // 帧时间
    last_frame_at: Instant,
}
```

`EngineHandle`（放进 manage）：
```rust
pub struct EngineHandle {
    pub tx:   tokio::sync::mpsc::Sender<EngineEvent>,
    pub join: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
}
```

### 2.3 Store 的 key 结构

二选一：

**方案 A：单 key 整体序列化**
```rust
store.set("config", serde_json::to_value(&app_config)?);
```
优点：原子性强，一次 deserialize 出整个 AppConfig
缺点：任何字段改了都全量序列化（但反正 plugin 内部也是全量序列化，无差别）

**方案 B：分桶 key**
```rust
store.set("session.settling_duration_ms", 5000);
store.set("channels.saturation",          json_value);
store.set("channels.color_temperature",   json_value);
store.set("channels.brightness",          json_value);
store.set("shortcuts.session_durations",  vec![25*60_000, 50*60_000, 90*60_000]);
```
优点：未来加字段无需迁移；前端可以单字段订阅
缺点：handle_command 时需要知道改的是哪个 key

**推荐 B**。命令本身就携带语义（"修改了 Saturation 通道的 settling curve"），按命令分发到对应 store key 反而更清晰。前端用 `getStore` + `onKeyChange` 也能得到精准订阅。

---

## Part 3 · 三个悬而未决问题的回应

### 3.1 子渲染器在其他线程出问题

#### 当前 Windows 实现的实际情况

`WindowsColorTransformer` **不开自己的线程**——它通过 `app.run_on_main_thread(closure)` 把闭包投递到 Tauri 主线程。这意味着：

- 没有"engine 拥有的子渲染器线程"在跑
- 所有 FFI 调用都在主线程（Tauri 事件循环线程）
- 调用结果通过 mpsc 反馈给 engine（`RendererEvent`）

#### 健康监测

如果主线程卡住（驱动 bug、第三方钩子等），症状是 engine 持续投递闭包但收不到任何 `RendererEvent` 回执。建议：

```rust
struct RendererHealthMonitor {
    outstanding_dispatches: u32,    // apply 时 +1，收到 RendererEvent 时 -1
    last_response_at:       Instant,
    state:                  HealthState, // Healthy | Degraded | Unresponsive
}
```

每帧 collect_events 时统计 outstanding；若 `now - last_response_at > 5s` 且 outstanding > 阈值，标记 `Degraded`，下一次 `apply` 跳过 dispatch（避免堆积闭包），并通过 `app.emit("renderer-degraded", ...)` 通知前端温和提示用户。

这部分**不放在 Batch 1**，作为 Batch 8/9 的可观测性增强。

#### 退出时主线程已卡

`renderer.try_shutdown` 用 `oneshot.recv_timeout(3s)` 等待。超时后：
- **不再 spawn 新线程做 cleanup**（你的担忧是对的，会引入 UB / 所有权竞争）
- 直接日志记录 + 跳过；让操作系统在进程退出时清理 FFI 资源（Magnification API 在进程退出时会自动释放）
- Windows 上即使 `MagSetFullscreenColorEffect(identity)` 没调用成功，进程退出后系统也会恢复屏幕

#### 未来的真线程子渲染器（声音）

约定接口：
```rust
trait SubRenderer: Send {
    fn apply(&mut self, frame: &LogicFrame, app: &AppHandle);
    fn try_shutdown(&mut self, app: &AppHandle, timeout: Duration) -> ShutdownResult;
}

enum ShutdownResult { Clean, TimedOut, AlreadyDead }
```

每个子渲染器自己管理其线程 + cancellation token。主渲染器（`Renderer`）调度多个子渲染器并并行 `try_shutdown`，对每个独立超时。

### 3.2 Engine 生命周期绑定到 App 生命周期

Tauri 没有现成的"绑定生命周期"概念，要靠**两端守门**：

```
[start]  Builder::setup(|app| { 
            spawn engine thread →
            app.manage(EngineHandle { tx, join })
            Ok(())
          })

[end]    app.run(|_, RunEvent::ExitRequested { api, .. }| {
            api.prevent_exit();
            spawn cleanup_thread:
                tx.send(Shutdown);
                join_handle.join();
                CLEANUP_DONE = true;
                app.exit(...);
          })
```

`app.manage(EngineHandle)` 本身把 EngineHandle 的所有权交给 Tauri 的 state map，Tauri 进程退出时它的 Drop 才会被调用。在 `EngineHandle::Drop` 里加防御性日志：

```rust
impl Drop for EngineHandle {
    fn drop(&mut self) {
        if self.join.lock().unwrap().is_some() {
            // 程序在没有走 ExitRequested 钩子的情况下被 drop —— 异常路径
            eprintln!("[EngineHandle] dropped without explicit shutdown");
        }
    }
}
```

#### Engine 线程 panic

`std::thread::JoinHandle::join()` 返回 `Result<(), Box<dyn Any + Send>>`。如果 engine 线程 panic：
- `join()` 返回 `Err(panic_payload)`
- cleanup_thread 拿到 `Err` 后，跳过对 engine 内部的清理，但需要**自行恢复屏幕**（panic 时 renderer 状态悬空）

```rust
match join_handle.join() {
    Ok(()) => { /* 正常 */ }
    Err(panic) => {
        log_panic(&panic);
        // best-effort: 直接在 cleanup_thread 调 FFI 恢复
        // Windows: 通过 run_on_main_thread 调 MagSetFullscreenColorEffect(identity)
        // 不依赖 engine 状态；FFI 本身是无状态的 reset
        let _ = app.run_on_main_thread(|| {
            unsafe { MagSetFullscreenColorEffect(&IDENTITY_EFFECT) };
        });
    }
}
```

这条路径不需要 Renderer 实例（因为 Renderer 已经在 panic 时一同消失了）—— 直接调 FFI 即可。

### 3.3 Tauri Channel 的生命周期

#### 关键事实

`tauri::ipc::Channel<T>` 的工作原理：
- 前端 invoke 命令时把 channel 作为参数传过来
- 后端持有的 `Channel<T>` 实质是一个内部路由 ID + AppHandle 的 weak ref
- `channel.send(...)` 会序列化 payload 并通过 IPC 投递到对应的前端 callback
- **窗口关闭后**：channel 仍然能 `send()` 不报错，但 payload 会静默丢失（前端 callback 不存在）
- **窗口再次打开**：旧 channel 永久失效，需要前端重新 invoke 一次注册命令拿新 channel

#### 推荐协议

```rust
enum EngineEvent {
    // ...
    RegisterProgressChannel(tauri::ipc::Channel<ProgressPayload>),
    ClearProgressChannel,
}

#[tauri::command]
async fn register_progress_channel(
    handle: State<'_, EngineHandle>,
    channel: tauri::ipc::Channel<ProgressPayload>,
) -> Result<(), String> {
    handle.tx.send(EngineEvent::RegisterProgressChannel(channel))
        .await.map_err(|e| e.to_string())
}
```

前端：
```js
// 窗口可见时（onMounted in Vue）
const ch = new Channel<ProgressPayload>();
ch.onmessage = (m) => updateUI(m);
await invoke('register_progress_channel', { channel: ch });

// 窗口隐藏 / 卸载时
await invoke('clear_progress_channel');
```

#### Engine 端

```rust
fn handle_engine_event(&mut self, ev: EngineEvent) {
    match ev {
        EngineEvent::RegisterProgressChannel(ch) => {
            self.progress_to_frontend = Some(ch);  // 覆盖旧 channel
        }
        EngineEvent::ClearProgressChannel => {
            self.progress_to_frontend = None;
        }
        // ...
    }
}

fn broadcast_progress(&mut self, now: Instant, state: &TimerState) {
    let payload = ProgressPayload::from(state);
    
    if let Some(ch) = &self.progress_to_frontend {
        let _ = ch.send(payload.clone());     // 静默忽略 send 失败
    }
    
    // tray 不依赖 frontend channel
    if now - self.last_tray_update >= TRAY_THROTTLE {
        tray::update_title(&self.app, &payload);
        self.last_tray_update = now;
    }
}
```

#### 设计性质

- **窗口隐藏不影响 tray 更新**：tray 走独立路径
- **窗口关闭后 send 失败但不 crash**：fire-and-forget
- **窗口重开后**：前端重新 register，覆盖旧 channel；之前丢失的进度无需补偿（只显示当前帧即可）
- **多窗口**：当前规划是单主窗口；如果未来有多窗口，可以扩展为 `Vec<Channel>` 但目前 YAGNI

---

## Part 4 · 审计中新发现的问题

### 4.1 `lib.rs` 当前不可编译

```rust
// 引用了不存在的模块和 commands
mod compositors;        // 文件不存在
session::*              // 文件存在但属于旧系统
commands::preview_frame // commands 模块根本没声明
```

**B0 必做**：清理旧模块声明 + 删旧文件 + 让 `pub fn run()` 退化到最小可编译形态。

### 4.2 `tray.rs` 的事件投递路径与新协议不一致

```rust
"take_break" => { let _ = app.emit("tray-take-break", ()); }
```

托盘菜单事件通过 `app.emit` 走前端再通过命令转发回后端——绕了一圈。新协议下应该直接：
```rust
"take_break" => {
    let handle = app.state::<EngineHandle>();
    let _ = handle.tx.try_send(EngineEvent::State(StateCommand::TakeBreakNow));
}
```

注意：tray 回调闭包需要 `Send + 'static`，但 `app.state::<T>()` 返回的 `tauri::State<T>` 不是 `'static`。需要在 setup_tray 阶段 clone 一份 `Sender<EngineEvent>` 进闭包：

```rust
pub fn setup_tray<R>(app: &AppHandle<R>, tx: Sender<EngineEvent>) -> tauri::Result<()> {
    // builder.on_menu_event(move |app, event| {
    //     let tx = tx.clone();
    //     match event.id.as_ref() { ... tx.try_send(...) }
    // })
}
```

### 4.3 `tauri.conf.json` 需要配置启动隐藏

用户需求"前端不弹出界面，除非按下快捷键或 tray 按钮"。需要：
```json
"windows": [{
    "label": "main",
    "visible": false,        // 启动时隐藏
    "skipTaskbar": true,     // 不出现在任务栏
    // ...
}]
```

托盘 click / 命令唤起时再 `window.show() + window.set_focus()`。`tray.rs` 当前已有此逻辑。

### 4.4 `TimerStateMachine.cerrent_elapsed_ms` 拼写错误 + 字段冗余

之前已列。每个 variant 内部已有 `elapsed_ms`，外层这个字段完全冗余。

### 4.5 `SensoryChannel::tick` 通道关闭时硬切到 neutral

```rust
if !self.switch_on {
    this_value = self.channel_type().neutral_value();
}
```

用户期望"通道关闭后渲染器自查 → 平滑 unload"。这里直接硬切到 neutral 会让 `current_value` 立刻变 `Changed(neutral)`，子渲染器拿到的就是直接跳到中性的指令——视觉上是突变。

**两种修复**：
- **方案 A**：通道关闭时不立即切，先把 target 切为 neutral 然后走一个 "FadeOff" 子状态（类似 Reverse）
- **方案 B**：保持现状（硬切），子渲染器自己做"fade-out 后 unload"插值

方案 A 更纯粹（"通道关闭"和"子渲染器卸载"语义分离），方案 B 更接近你最初描述的"渲染器自查关闭"。但方案 B 在 Windows 上很尴尬：Magnification API 一旦应用矩阵就是即时生效，"fade-out"需要在子渲染器内部跑一个本地的小状态机。

**建议**：v1 先用方案 A 的简化版——通道关闭命令不立即生效，而是发起一个"Reverse"动画，结束后才真正关闭。但这需要状态机感知"哪些通道在 fade-out"。先列为 TODO，v0 用方案 B 硬切（用户能接受的回退）。

### 4.6 `Update<ChannelValue>` 的 Unchanged 传播

之前你说会自己修。需要修的就是当前的：

```rust
if this_value == *self.current_value.get_value() {
    self.current_value = Update::Unchanged(this_value);
}
```

这里没有考虑"上一帧是 Unchanged，这一帧值依然没变"应该保持 Unchanged 的累积语义——目前的实现已经是对的（只看值不看 tag），所以应该没问题。我之前的担忧是多余的，这条不算 issue。

### 4.7 `Renderer` 的 `Sender` 类型需要统一

当前 `Sender<RendererEvent>`，新方案要换成 `Sender<EngineEvent>`。涉及：
- `renderer/mod.rs::RendererEventSending` trait
- `renderer/windows.rs` 所有 sender 类型签名
- `Renderer::new` 构造签名

机械改动，但如果忘了 macOS 那边（虽然现在是空壳）也要同步。

### 4.8 `observability.rs` 走 ManagedObservability 与新原则冲突

当前实现需要 `app.state::<ManagedObservability>()` + `Mutex` 跨线程访问。新原则下应该是 engine 独占：

```rust
pub struct Observability {
    file_path: Option<PathBuf>,
}

impl Observability {
    pub fn log_renderer_event(&mut self, event: &RendererEvent) { ... }
    pub fn log_state_command(&mut self, cmd: &StateCommand) { ... }
}
```

Engine 字段：`observability: Observability`。命令处理时 engine 直接调 `self.observability.log_*(...)`。完全不放 manage。

如果未来其他线程需要日志（panic hook），那个路径独立写一个 `eprintln!` 或写文件即可，不通过 engine。

### 4.9 三个时长快捷键的命令复用

用户需求"允许设定若干个（例如三个）快捷键对应不同时长"——已有的 `StateCommand::StartSession { target_duration_ms }` 完全覆盖。前端配置三个 duration，通过 global-shortcut 注册三个快捷键，每个回调发对应的 StartSession 即可。**不需要新命令**。

### 4.10 强制 reset 命令缺失

用户需求"允许用户直接强制初始化所有 renderer，归零重启"。需要新命令：
```rust
enum StateCommand {
    // ...
    ForceResetRenderer,    // 应用 identity 矩阵 + 重置子渲染器内部状态
}
```

Engine 收到后调 `renderer.try_reset(&app)`。

### 4.11 Panic 时通知用户的"温和"方式

用户需求"提示方式不应当太『扎眼』或『刺耳』"。我之前提过两种途径：
- panic hook 弹窗 — 太扎眼
- 写入 store 的 `last_crash` key，**下次启动时**前端检测并显示一个温和 banner — 推荐

```rust
// panic hook
let _ = store.set("system.last_crash", json!({
    "timestamp": now,
    "phase": phase,
    "message": panic_msg,
}));
let _ = store.save();   // 同步落盘

// 前端启动时
if (await store.get('system.last_crash')) {
    showQuietBanner('上次运行异常退出，已恢复正常状态');
    await store.delete('system.last_crash');
}
```

`tauri-plugin-store` 的 `Store<R>` 是 `Send + Sync`，可以放进 `panic::set_hook` 闭包（`Arc<Store<R>>` clone 一份进去）。但 panic hook 在哪个线程跑取决于 panic 位置——可能是 engine thread，可能是主线程，也可能是子渲染器线程。`store.save()` 同步阻塞 + 同步 IO，在任何线程都能跑。

---

## Part 5 · 最终实现计划（融合所有结论）

### 状态机与命令的最终形态

```rust
// events/timer_state_commands.rs
pub enum StateCommand {
    StartSession { target_duration_ms: u64 },
    TakeBreakNow,
    StopSession,
    EnterPreview,
    ExitPreview,
    UpdatePreviewProgress { progress: f64 },
    ForceResetRenderer,
}

// events/mod.rs (新增顶层枚举)
pub enum EngineEvent {
    State(StateCommand),
    Channel(ChannelCommand),
    Renderer(RendererEvent),
    RegisterProgressChannel(tauri::ipc::Channel<ProgressPayload>),
    ClearProgressChannel,
    Shutdown,
}

// engine/frame_events.rs
pub struct FrameEvents {
    pub state_commands:    Vec<StateCommand>,
    pub channel_commands:  Vec<ChannelCommand>,
    pub shutdown_requested: bool,
    pub just_transited:    bool,
    pub switch_changed:    bool,
    pub force_reset:       bool,
}
```

### 主循环骨架

```rust
impl<R: Runtime> Engine<R> {
    fn run(mut self) {
        loop {
            let now = Instant::now();
            let dt_ms = now.saturating_duration_since(self.last_frame_at).as_millis() as u64;
            self.last_frame_at = now;
            
            let mut fe = self.collect_events();
            if fe.shutdown_requested {
                self.do_shutdown();
                break;
            }
            if fe.force_reset {
                self.renderer.try_reset(&self.app);
            }
            
            // ── 状态机 ────────────────────────
            self.timer.tick(dt_ms, &mut fe, &self.config);
            
            // ── 通道（消费 channel_commands + dispatch 持久化） ────
            for cmd in fe.channel_commands.drain(..) {
                let target = self.channels[cmd.channel_type()];
                if target.handle_command(&cmd) {
                    self.persist_channel_change(cmd.channel_type());
                }
            }
            for ch in self.channels.iter_mut() {
                ch.tick(self.timer.state(), &fe);
            }
            
            // ── 副作用阶段 ────────────────────
            let switch_states = self.channels.switch_states();
            if fe.switch_changed {
                self.renderer.switch_subrenderer_states(switch_states, &self.app);
            }
            let logic_frame = Arc::new(self.channels.logic_frame());
            self.renderer.render(logic_frame, &self.app);
            
            // ── 进度同步 ──────────────────────
            self.broadcast_progress(now, &self.timer.state());
            if fe.just_transited {
                let _ = self.app.emit("phase_changed", self.timer.state().label());
            }
            
            // ── 自适应睡眠 ────────────────────
            let interval = self.timer.recommended_tick_interval();
            let elapsed  = now.elapsed();
            if elapsed < interval {
                std::thread::sleep(interval - elapsed);
            }
        }
    }
    
    fn collect_events(&mut self) -> FrameEvents {
        let mut fe = FrameEvents::default();
        while let Ok(ev) = self.event_rx.try_recv() {
            match ev {
                EngineEvent::State(c)    => fe.state_commands.push(c),
                EngineEvent::Channel(c)  => fe.channel_commands.push(c),
                EngineEvent::Renderer(r) => self.observability.log_renderer_event(&r),
                EngineEvent::RegisterProgressChannel(ch) => self.progress_to_frontend = Some(ch),
                EngineEvent::ClearProgressChannel        => self.progress_to_frontend = None,
                EngineEvent::Shutdown    => fe.shutdown_requested = true,
            }
        }
        fe
    }
    
    fn do_shutdown(&mut self) {
        self.observability.log_shutdown_start();
        self.renderer.try_shutdown(&self.app);   // 同步等 3s
        let _ = self.store.save();               // 同步阻塞落盘
        self.observability.log_shutdown_complete();
    }
}
```

### 实施批次（重排）

#### B0 · 让代码恢复编译 + 引入依赖

- [x] 删除 `compositor.rs`、`compositors.rs`（不存在但有引用）、`platform.rs`、`phase.rs`、`session.rs`、`old_commands.rs`
- [x] `lib.rs` 删 `mod compositor; mod compositors; mod platform; mod phase; mod session;` 与对应 manage/handler
- [x] 退化 `pub fn run()`：
  ```rust
  pub fn run() {
      tauri::Builder::default()
          .setup(|app| { tray::setup_tray(app.handle())?; Ok(()) })
          .run(tauri::generate_context!())
          .expect("...");
  }
  ```
- [x] `Cargo.toml` 添加：
  ```toml
  tauri-plugin-store = "2"
  tauri-plugin-global-shortcut = "2"
  tauri-plugin-autostart = "2"
  tokio = { version = "1", features = ["sync", "time", "rt-multi-thread"] }
  ```
- [x] `cargo check` 通过

**里程碑 M1**：能编译，`tauri dev` 能启动空 app + tray

#### B1 · 事件骨架与 Engine 空跑

- [x] `events/timer_state_commands.rs` 加 `EnterPreview/ExitPreview/UpdatePreviewProgress/ForceResetRenderer`
- [x] `events/mod.rs` 新增 `EngineEvent` 枚举
- [x] `engine.rs` 拆为 `engine/mod.rs` + `engine/frame_events.rs`
- [x] `FrameFlags` 升级为 `FrameEvents`
- [x] `engine/handle.rs` 定义 `EngineHandle { tx, join }` + `Drop` 防御日志
- [x] `lib.rs::setup` 中：构造 `mpsc::channel(256)` → spawn engine thread → `app.manage(EngineHandle)`
- [x] Engine `run` 仅有空循环 + collect_events + Shutdown 退出

**里程碑 M2**：engine thread 空跑，关闭信号能让它退出

#### B2 · TimerStateMachine 升级

- [x] 删除 `cerrent_elapsed_ms` 字段及 getter
- [x] 新增 `TimerState::Preview { progress: f64 }`
- [x] 新增 `recommended_tick_interval()`
- [x] `handle_command`：Preview 命令分支、StartSession 在 Preview 下忽略 + warn
- [x] `update`：Preview 分支 no-op + 注释
- [x] `_ => {}` 加 warn log
- [x] 单元测试

#### B3 · Channels 跨平台统一 + Preview 分支

- [x] 合并 `define_channels!`（去掉 cfg 分裂）
- [x] `ChannelType::is_available_on_this_platform()`
- [x] 把 `calculate_this_value` 重构为接受 `progress: f64` 的纯函数
- [x] `tick` 加 Preview 分支
- [x] `ChannelFuncParamsMatrix::Index<TimerState>` 把 Preview 映射到 PROGRESS 槽
- [x] 通道关闭硬切到 neutral —— 留 TODO 注释（v1 改为 fade-out）

#### B4 · Renderer 完善

###### B4.1 Windows
- [ ] `Sender<RendererEvent>` → `Sender<EngineEvent>`
- [ ] `try_shutdown` 实现：oneshot + run_on_main_thread + 3s 超时 + 恢复 identity + uninit
- [ ] `try_reset`：dispatch identity + 重置 cached_color_transform_matrix
- [ ] `switch_subrenderer_states`：当前单子渲染器，no-op + 注释
- [ ] 健康监测（outstanding_dispatches）—— 推迟到 B8

###### B4.2 macOS（独立 PR）
- [ ] 从旧 `platform.rs::mod macos` 迁移 FFI
- [ ] 实现 Rendering trait
- [ ] 渲染时忽略 Saturation 通道

#### B5 · Persistence (走 Store 插件)

- [ ] `Cargo.toml` 已加 `tauri-plugin-store`
- [ ] `lib.rs::setup` 中：
  ```rust
  let store = StoreBuilder::new(app, "config.json")
      .auto_save(Duration::from_secs(2))
      .default("channels", default_channels_json)
      .default("session.settling_duration_ms", 5_000)
      .default("session.reverse_duration_ms", 2_000)
      .default("shortcuts.session_durations", vec![25*60_000u64, 50*60_000, 90*60_000])
      .build()?;
  ```
- [ ] 重写 `configs.rs::AppConfig`：从 store 读取构造，不持有 store
- [ ] Engine 持有 `Arc<Store<R>>`
- [ ] 命令处理时调 `store.set("channels.saturation", value)` 等
- [ ] Shutdown 路径调 `store.save()` 同步阻塞

#### B6 · Shutdown 路径

- [ ] `lib.rs` 改用 `Builder::build()` + `app.run(|app, event| ...)` 模式
- [ ] `static CLEANUP_DONE: AtomicBool`
- [ ] `RunEvent::ExitRequested` 处理：`prevent_exit` + spawn cleanup_thread
- [ ] cleanup_thread 内：`tx.send(Shutdown)` + `engine.join()` + 翻 atomic + `app.exit()`
- [ ] Engine `do_shutdown` 实现
- [ ] panic hook：写入 `store.set("system.last_crash", ...)` + `store.save()`

#### B7 · Tauri 挂载

###### B7.1 Commands
- [ ] 重写 `commands.rs`（不是 old_commands.rs）：所有命令薄转发到 EngineHandle.tx
- [ ] `register_progress_channel` / `clear_progress_channel`
- [ ] `get_available_channels` 返回平台可用通道列表
- [ ] `lib.rs::invoke_handler` 注册新命令

###### B7.2 Tray
- [ ] `setup_tray` 接收 `Sender<EngineEvent>`，菜单回调改为直接发 EngineEvent
- [ ] `update_tray_title` 显示倒计时
- [ ] `update_tray_menu` 根据 phase 启用/禁用菜单项
- [ ] tray icon 显示进度（圆形进度条图标）—— v1 简化为文字 title，圆形 icon 留 TODO

###### B7.3 全局快捷键
- [ ] `tauri-plugin-global-shortcut` 集成
- [ ] 注册 N 个快捷键（N 由 store 配置驱动）：
  - 三个 StartSession 快捷键（不同时长）
  - 一个 TakeBreakNow（peek-to-peak settling）
  - 一个 StopSession（reverse）
  - 一个 ForceResetRenderer
- [ ] 回调发对应 EngineEvent

###### B7.4 自启动
- [ ] `tauri-plugin-autostart` 集成，默认开启
- [ ] 设置面板提供开关

###### B7.5 启动隐藏
- [ ] `tauri.conf.json`：`"visible": false`
- [ ] tray click / 命令唤起时 `window.show()`

#### B8 · Observability 现代化

- [ ] 删除 `ManagedObservability`
- [ ] `Observability { file_path }` 由 engine 独占
- [ ] panic hook 集成
- [ ] 渲染器健康监测

#### B9 · 保留 TODO（不阻塞发布）

- [ ] 通道关闭走 fade-out 而非硬切
- [ ] Tray 圆形进度图标
- [ ] 渲染器降级时前端温和提示
- [ ] 子渲染器健康监测的 `Degraded` 状态机
- [ ] Settling/Reverse 智能合并（用户连按两次快捷键时不重叠）

### 里程碑映射

| 里程碑 | 完成 | 用户能做的事 |
|---|---|---|
| **M1** 能编译 | B0 | tauri dev 启动空 app |
| **M2** Engine 空跑 | B1 | engine 线程能起能停 |
| **M3** 业务跑通 | B2~B5 | 命令能改通道，Windows 渲染输出正常 |
| **M4** 优雅退出 | B6 | tray Quit 不留色卡，配置落盘 |
| **M5** 可用 | B7 | 前后端联通，快捷键工作，开机自启 |
| **M6** 健壮 | B8 + B4.2 | macOS 也能跑，崩溃可恢复 |

---

## Part 6 · 还需要你拍板的几个点

1. **Store key 结构**：方案 A（单 key）还是 B（分桶）？我推荐 B
2. **通道关闭语义**：v0 用硬切到 neutral（简单但视觉跳变），v1 改为 fade-out？还是从 v0 就上 fade-out？
3. **快捷键集**：三个 StartSession + TakeBreakNow + StopSession + ForceResetRenderer 是否够？还是再加 EnterPreview / ExitPreview？
4. **Tray icon 进度可视化**：v0 仅 title 文字（如 "25:13 left"），v1 圆形进度图标？还是从 v0 就上图标？
5. **panic 恢复策略**：写 store.last_crash + 下次启动 banner——同意吗？还是你希望直接静默不提示？
6. **Cargo.toml 是否加 `tracing`**：现有 observability 是手撸 jsonl，要不要换成 tracing 生态？

只要 1~5 拍板（6 可以不动），就能开干 B0。

# User
通道关闭时渐进过渡是一个potential feature，不一定需要：因为设想的开关场景在配置各个通道，此时反而需要迅速反应，除非将来需求包括在计时过程中开关通道，否则没必要。
SensoryChannels需要包装，也像timer一样开放tick, which includes handdle_command and update。
Engine不被App持有，不放入State，因为它所有权要被转给engine thread而且要求可修改；EngineHandle被放入manage；engine.run(self)为其签名，直接转移所有权，这样是否比&mut self好。
EngineHandle没必要加防御性日志，它足够简单，真正要concern的是engine先挂掉了而没办法知道；与其让cleanup_thread费力去拿到各个api，不如让Engine实现Drop，这样它可以用自己持有的AppHandle来best effort清理，但是还没有想清楚清理逻辑会不会有二次panic的可能。
observability走tracing生态，保存位置有没有什么现成的log默认保存位置变量，依不同平台而位置不同？
ForceResetRenderer改名为ForceReset，它把TimerState更改为Idle的同时要求Renderer重启所有subrenderer——即终止所有当前渲染并直接重新拉起来各自的api。所以它是一个全局Command，不要放在StateCommand下。ProgressChannnel的两个Command也再包一层enum，目的为与State, Channel, Renderer统一。
panic除了使用last_crash，也在tray那里提示——本次crash和上一次crash了都提示，具体怎么提示还没设计。
self.timer.recommended_tick_interval()改为self.recommended_tick_interval()，因为具体由谁决定尚未明确，不要写死接口。现在直接写死争取每秒六十帧。
为什么要用tokio的rt_multi-thread？
Preview添加快捷键，此外也添加拉起前端窗口和关闭前端窗口的快捷键。

# Agent
## 逐点回应

---

### 1. 通道关闭：硬切方案最终确认

去掉计划里的 fade-out TODO。`SensoryChannel::tick` 的现有逻辑（`!switch_on → neutral_value`）保留。未来若需求扩展到"计时中开关通道"再上 fade-out。

---

### 2. SensoryChannels 包装

和 `TimerStateMachine` 对称，包装成有 `tick(state, &mut FrameEvents)` 方法的结构体：

```rust
// channels.rs
pub struct SensoryChannelsSystem {
    channels: SensoryChannels,   // [SensoryChannel; SENSORY_CHANNEL_COUNT]
}

impl SensoryChannelsSystem {
    pub fn new(configs: AllChannelsConfigs) -> Self { ... }
    
    pub fn tick(
        &mut self,
        state: TimerState,
        fe: &mut FrameEvents,
        store: &Store<R>,                  // 命令处理时直接调 store.set
    ) {
        // ── handle_command 阶段 ───────────
        for cmd in fe.channel_commands.drain(..) {
            let target_type = cmd.channel_type();
            let prev_switch = self.channels[target_type].is_switch_on();
            
            self.channels[target_type].handle_command(&cmd);
            
            // 检测开关变化 & 持久化
            let new_switch = self.channels[target_type].is_switch_on();
            if prev_switch != new_switch {
                fe.switch_changed = true;
            }
            self.persist(target_type, store);
        }
        
        // ── update 阶段 ──────────────────
        for ch in self.channels.iter_mut() {
            ch.tick(state, fe);
        }
    }
    
    pub fn logic_frame(&self) -> LogicFrame { ... }
    pub fn switch_states(&self) -> ChannelSwitchStates { ... }
    
    fn persist(&self, ch_type: ChannelType, store: &Store<R>) {
        let key = format!("channels.{}", ch_type.serde_key());
        let _ = store.set(key, serde_json::to_value(&self.channels[ch_type].persist()).unwrap());
    }
}
```

`ChannelCommand` 需要新增一个 `pub fn channel_type(&self) -> ChannelType` 辅助方法（目前每个 variant 都带 channel_type 字段或可推断）。

---

### 3. `engine.run(self)` 强烈推荐

`fn run(self)` 比 `fn run(&mut self)` 优势明显：

| 方面 | `run(self)` | `run(&mut self)` |
|---|---|---|
| spawn 模式 | `thread::spawn(move \|\| engine.run())` 自然 | 需要外部容器持有 + Mutex 之类 |
| Drop 触发 | run 返回后立即 drop self | 调用方决定何时 drop |
| panic unwinding | unwinding 经过 run 边界时自动 drop | 取决于外部容器 |
| 配合 Engine::Drop | 完美：Drop 是真正的清理终点 | Drop 在外部决定，时机不可控 |
| 可读性 | "这个函数拥有并消耗 engine" 语义清晰 | 需要看上下文才能判断生命周期 |

签名敲定：
```rust
impl<R: Runtime> Engine<R> {
    pub fn run(mut self) {
        loop { ... }
    }
}

// lib.rs::setup
let engine = Engine::new(...)?;
let join = std::thread::spawn(move || engine.run());
app.manage(EngineHandle { tx, join: Mutex::new(Some(join)) });
```

---

### 4. Engine 实现 Drop + 二次 panic 防护

#### 4.1 二次 panic 的真实风险

Rust 规则：**Drop 在 panic unwinding 路径上再次 panic → 进程 abort（不是 unwinding 而是直接终止）**。这是 Drop 实现里最致命的禁忌。

#### 4.2 安全模式

```rust
use std::panic::{catch_unwind, AssertUnwindSafe};

pub struct Engine<R: Runtime> {
    // ... 其他字段
    cleaned_up: bool,    // 正常 shutdown 路径会置 true
}

impl<R: Runtime> Engine<R> {
    fn do_shutdown(&mut self) {
        if self.cleaned_up { return; }
        
        tracing::info!("engine shutdown begin");
        self.renderer.try_shutdown(&self.app);     // 内部 timeout，不 panic
        let _ = self.store.save();                  // Result，不 panic
        tracing::info!("engine shutdown complete");
        
        self.cleaned_up = true;
    }
    
    pub fn run(mut self) {
        loop {
            // ... 主循环
            if fe.shutdown_requested {
                self.do_shutdown();
                break;
            }
        }
        // self 在此 drop；cleaned_up = true，Drop::drop 走快路径
    }
}

impl<R: Runtime> Drop for Engine<R> {
    fn drop(&mut self) {
        if self.cleaned_up {
            return;     // 正常路径：shutdown 已经做过了，无事可做
        }
        
        // 异常路径：engine 没走 do_shutdown 就被 drop（panic unwinding）
        // 关键：catch_unwind 包住所有清理动作，避免二次 panic
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // 所有调用都用 .ok() / let _ = ...，不允许任何 unwrap/expect
            self.renderer.try_shutdown(&self.app);
            
            // 写 panic 标志到 store（panic_message 由 panic_hook 提前写入）
            let _ = self.store.save();
            
            // 直接用 std::io 写一行简单日志，不依赖 tracing（subscriber 可能已挂）
            let _ = writeln!(std::io::stderr(), "[Engine::drop] panic recovery cleanup done");
        }));
    }
}
```

#### 4.3 仍需 panic hook 写 last_crash

Drop 是 panic 之后跑的，拿不到 panic message。所以 panic hook 仍然需要在最早期写 last_crash：

```rust
// lib.rs::setup
let store_for_hook = store.clone();   // Arc<Store<R>> cheap clone
let app_for_hook = app.handle().clone();
std::panic::set_hook(Box::new(move |info| {
    let msg = format!("{info}");
    let _ = store_for_hook.set("system.last_crash", json!({
        "timestamp_ms": now_ms(),
        "message": msg,
        "thread": std::thread::current().name().unwrap_or("unknown").to_string(),
    }));
    let _ = store_for_hook.save();
    
    // tray 标题改为告警态（不弹窗）
    if let Some(tray) = app_for_hook.tray_by_id("main") {
        let _ = tray.set_title(Some("⚠ Erode (recovering)"));
    }
}));
```

panic hook 跑在出 panic 的线程上下文里，不依赖 engine。

#### 4.4 Engine 既被 Drop 又被外部 join 的协同

cleanup_thread 调 `engine_thread.join()`：
- 正常路径：engine 内 do_shutdown → return → drop → join() 返回 Ok(())
- panic 路径：engine 内 panic → unwinding → drop（catch_unwind 兜底清理）→ thread 退出 → join() 返回 Err(panic_payload)

两条路径下 cleanup 都被执行，且最多执行一次（cleaned_up flag）。

---

### 5. tracing 跨平台日志位置

#### 5.1 没有"现成全局变量"

`tracing` 自己没有平台默认路径概念。需要通过 Tauri 的 `app_handle.path()` API 取得：

```rust
let log_dir = app.path()
    .app_log_dir()
    .or_else(|_| app.path().app_local_data_dir())
    .map_err(|e| ...)?;
std::fs::create_dir_all(&log_dir)?;
```

各平台的实际位置：

| 平台 | `app_log_dir()` |
|---|---|
| Windows | `%LOCALAPPDATA%\<bundle_id>\logs` |
| macOS | `~/Library/Logs/<bundle_id>` |
| Linux | `$XDG_DATA_HOME/<bundle_id>/logs` 或 `~/.local/share/<bundle_id>/logs` |

#### 5.2 推荐技术栈

```toml
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt", "json"] }
tracing-appender = "0.2"
```

```rust
// lib.rs::setup
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

let log_dir = app.path().app_log_dir()
    .or_else(|_| app.path().app_local_data_dir())?;
std::fs::create_dir_all(&log_dir)?;

let file_appender = tracing_appender::rolling::daily(&log_dir, "erode.log");
let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

tracing_subscriber::registry()
    .with(EnvFilter::from_default_env()
        .add_directive("erode_app_lib=info".parse()?))
    .with(fmt::layer().with_writer(non_blocking).json())
    .with(fmt::layer().with_writer(std::io::stderr))   // 开发期 stderr
    .init();

// 注意：guard 必须存活到进程结束，否则后台 flush 线程被 drop，未 flush 的日志丢失
app.manage(LogGuard(guard));    // 包一层放进 manage 即可
```

`tracing-appender` 的 `non_blocking` 是性能关键——日志写盘走专用线程，不阻塞 engine 主循环。

#### 5.3 Engine 内的使用

`Observability` 结构体可以彻底废弃，engine 直接用 `tracing::info!()` / `error!()` / `warn!()`。事件结构化字段：

```rust
tracing::info!(
    target: "renderer",
    sub_renderer = %event.sub_renderer_name,
    "render successful"
);
```

---

### 6. ForceReset 提升为全局命令

```rust
pub enum EngineEvent {
    State(StateCommand),       // Timer 命令
    Channel(ChannelCommand),   // 通道命令
    Renderer(RendererEvent),   // 渲染回执
    Progress(ProgressCommand), // 进度 channel 注册/清除（见 §7）
    ForceReset,                // 全局
    Shutdown,                  // 全局
}
```

执行语义：
```rust
// engine main loop
EngineEvent::ForceReset => fe.force_reset = true,

// 在 collect_events 之后、timer.tick 之前
if fe.force_reset {
    self.timer.transit_to(TimerState::Idle);    // 强制 Idle
    fe.just_transited = true;
    self.renderer.try_reset(&self.app);          // 重启所有 subrenderer
    // 注意：channels.tick 之后会自然产出 neutral 值
    //       因为状态是 Idle，所有通道都返回 neutral
    //       下一帧 renderer.render 会把 neutral 推下去
}
```

关于"重启所有 subrenderer"的语义：
- Windows 单子渲染器：`MagSetFullscreenColorEffect(identity)` + `MagUninitialize` + 标记 `magnification_initialized = false`
- 下一次 render 调用时会自动 `MagInitialize`
- 用户感受：屏幕短暂回到原色（identity 矩阵生效），然后通道值产出 neutral 维持原色，无可见跳跃

`StateCommand::ForceResetRenderer` 从枚举中移除。

---

### 7. ProgressCommand 包装 + 完整 EngineEvent

```rust
// events/progress_commands.rs (新文件)
pub enum ProgressCommand {
    RegisterChannel(tauri::ipc::Channel<ProgressPayload>),
    ClearChannel,
}

// events/mod.rs
pub enum EngineEvent {
    State(StateCommand),
    Channel(ChannelCommand),
    Renderer(RendererEvent),
    Progress(ProgressCommand),
    ForceReset,
    Shutdown,
}
```

四个分类协议层 + 两个全局 → 整齐。

---

### 8. Panic 提示策略：tray 双信号

```
panic 发生时（panic hook）:
  1. store.set("system.last_crash", { timestamp, message, thread })
  2. store.save() 同步阻塞
  3. tray.set_title("⚠ Erode (recovering)")
  4. （v1）tray icon 切换为告警态图标

下次启动时（lib.rs::setup）:
  if let Some(crash) = store.get("system.last_crash") {
      tray.set_tooltip(format!("Recovered from crash on {}", crash.timestamp));
      // 或者通过 tray.set_title("⚠ Last run crashed") 持续展示直到用户清除
  }
  
  // 用户右键 tray → "Acknowledge crash" 菜单项 → 清除
  if menu_event == "ack_crash" {
      store.delete("system.last_crash");
      tray.set_title("Idle");
  }
```

具体提示文案、图标设计、用户清除流程留待 UI 设计阶段。代码层面只需保证：
- panic hook 能改 tray title（拿到 AppHandle clone）
- 启动时检查 store.last_crash
- 提供清除入口

---

#### 9. `recommended_tick_interval` 归 Engine + 60fps

接口移到 Engine：
```rust
impl<R: Runtime> Engine<R> {
    fn recommended_tick_interval(&self) -> Duration {
        // 60 fps 写死；未来根据 timer 状态分级
        Duration::from_micros(16_667)
    }
}
```

注意点：
- **Idle 状态下也 60fps 跑空循环**：每秒 60 次 try_recv + 一些拷贝。CPU 0.5~1%，可接受。
- **sleep 漂移**：Windows 默认 sleep 精度 15.6ms，在 16.67ms 下不稳定。如果出现帧不均匀的视觉感受，需要：
  - 调用 `timeBeginPeriod(1)` 提升系统计时器精度（但有副作用：影响其他进程）
  - 或者改用 spin-wait + sleep 组合
  - **v0 不动**，跑起来观察。如果有抖动再处理，TODO 记一笔。
- **Preview 状态：** 用户拖 progress 滑块时也是 60fps，能跟上输入

---

### 10. 我之前 `rt-multi-thread` 是错的

你问得对。重新分析：

| 操作 | 需要 tokio runtime？ |
|---|---|
| `tokio::sync::mpsc::Sender` 类型本身 | 需要 `tokio` feature `sync` |
| `tx.try_send(...)` | ❌ 不需要 runtime |
| `tx.blocking_send(...)` | ❌ 不需要 runtime（且不能在 runtime 内调） |
| `tx.send(...).await` | ✅ 需要 runtime（但 `#[tauri::command] async fn` 已经在 Tauri 自带的 runtime 里跑） |
| `rx.try_recv()` | ❌ 不需要 |
| `rx.blocking_recv()` | ❌ 不需要 |

**Engine 主循环**：纯 `std::thread`，用 `rx.try_recv()`，不需要 runtime。
**Tauri commands**：跑在 Tauri 内置 runtime（multi-thread），用 `tx.send(...).await`，runtime 已由 Tauri 提供。

我们自己的 `Cargo.toml` 只需要：
```toml
tokio = { version = "1", features = ["sync"] }
```

`rt-multi-thread` / `rt` / `time` / `macros` 都不需要。`time` 是之前用 `tokio::time::interval` 等用的，现在用 `std::thread::sleep` + `Instant` 也用不上。

修正之前计划。

---

### 11. 快捷键集 + 窗口控制

新增的全局快捷键：

| 快捷键 | 走 Engine？ | 命令 |
|---|---|---|
| 启动 25 分钟 session | ✅ | `EngineEvent::State(StartSession { 25*60_000 })` |
| 启动 50 分钟 session | ✅ | 同上，时长不同 |
| 启动 90 分钟 session | ✅ | 同上 |
| TakeBreakNow（peak settling） | ✅ | `EngineEvent::State(TakeBreakNow)` |
| StopSession（reverse） | ✅ | `EngineEvent::State(StopSession)` |
| EnterPreview | ✅ | `EngineEvent::State(EnterPreview)` |
| ExitPreview | ✅ | `EngineEvent::State(ExitPreview)` |
| ForceReset | ✅ | `EngineEvent::ForceReset` |
| **ShowMainWindow** | ❌ | 直接闭包内 `window.show() + set_focus()` |
| **HideMainWindow** | ❌ | 直接闭包内 `window.hide()` |

ShowMainWindow / HideMainWindow 不影响 Engine 任何状态——纯 OS 窗口操作。强行走 Engine 反而引入：
- 一次额外的事件序列化/反序列化
- 主线程跨线程跨等待（窗口操作必须在主线程）
- 没有任何业务收益

直接在快捷键回调内：
```rust
shortcut_handler(move |app, shortcut, event| {
    match shortcut.id() {
        "show_main" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "hide_main" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.hide();
            }
        }
        // ...其他都走 engine
        id => {
            if let Some(ev) = id_to_event(id) {
                let handle = app.state::<EngineHandle>();
                let _ = handle.tx.try_send(ev);
            }
        }
    }
});
```

---

## 修订后的关键结构汇总

### EngineEvent
```rust
pub enum EngineEvent {
    State(StateCommand),
    Channel(ChannelCommand),
    Renderer(RendererEvent),
    Progress(ProgressCommand),
    ForceReset,
    Shutdown,
}

pub enum StateCommand {
    StartSession { target_duration_ms: u64 },
    TakeBreakNow,
    StopSession,
    EnterPreview,
    ExitPreview,
    UpdatePreviewProgress { progress: f64 },
    // ForceResetRenderer 已移除，提升为全局
}

pub enum ProgressCommand {
    RegisterChannel(tauri::ipc::Channel<ProgressPayload>),
    ClearChannel,
}
```

### FrameEvents
```rust
pub struct FrameEvents {
    pub state_commands:    Vec<StateCommand>,
    pub channel_commands:  Vec<ChannelCommand>,
    pub shutdown_requested: bool,
    pub force_reset:       bool,
    pub just_transited:    bool,
    pub switch_changed:    bool,
}
```

### Engine 字段（最终）
```rust
pub struct Engine<R: Runtime> {
    timer:    TimerStateMachine,
    channels: SensoryChannelsSystem,
    renderer: Renderer,
    
    event_rx: tokio::sync::mpsc::Receiver<EngineEvent>,
    
    app:      AppHandle<R>,
    store:    Arc<Store<R>>,
    
    progress_to_frontend: Option<tauri::ipc::Channel<ProgressPayload>>,
    last_tray_update:     Instant,
    
    last_frame_at: Instant,
    cleaned_up:    bool,    // Drop 守门
}
```

### Cargo.toml 增量（最终）
```toml
tokio = { version = "1", features = ["sync"] }
tauri-plugin-store = "2"
tauri-plugin-global-shortcut = "2"
tauri-plugin-autostart = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt", "json"] }
tracing-appender = "0.2"
```

---

## 实施计划微调

只有少量增量，框架不变：

- **B0** 不变（清理旧文件 + 加依赖）
- **B1** EngineEvent 用 6 分类版（含 Progress / ForceReset / Shutdown）
- **B3** 加 `SensoryChannelsSystem` 包装结构
- **B5** 不写 `Persistence` 结构（store 自带 debounce），engine 持有 `Arc<Store<R>>`，命令处理时直接 `store.set`
- **B6** Engine::Drop with catch_unwind + cleaned_up flag；panic hook 写 last_crash + tray title 告警
- **B7** 快捷键里 ShowMainWindow/HideMainWindow 走闭包直连，不入 Engine
- **B8** observability 模块**完全删除**，全部替换为 tracing macros；保留 `LogGuard` 包装放 manage

---

## 还需要确认的最后两点

1. **Store key 结构**：方案 A（单 `config` key）vs B（按 `channels.<type>` / `session.*` 等分桶）。我推荐 B，但你尚未明示，请确认。
2. **快捷键默认值是否硬编码**：三个 session 时长（25/50/90 分钟）写在代码里，还是从 store 读？后者用户可改但启动时若 store 为空有默认值。建议从 store 读 + StoreBuilder.default 提供默认值。

确认 1 + 2 后即可开 B0。

# User
```rust
            // 检测开关变化 & 持久化
            let new_switch = self.channels[target_type].is_switch_on();
            if prev_switch != new_switch {
                fe.switch_changed = true;
            }
```
SensoryChannelsSystem不要这样，直接根据当前cmd是否是ToggleSwitch来判断，如果有，直接设置fe.switch_changed为true。
TimerStateMachine和SensoryChannelsSystem全都实现一个reset方法，timer是transit_to Idle，而SensoryChannelsSystem则是把所有current_value恢复为中立值。这两个方法在if fe.force_reset {...}块中调用。
store key采取简单多桶：显然config和last_crash不放同一个桶，但不同种类channel是否放一个桶待定，要取决于它的持久化逻辑怎样好写。快捷键默认值从store中读取。
睡眠TODO记录若有都懂则引入第三方库：https://crates.io/crates/spin_sleep 