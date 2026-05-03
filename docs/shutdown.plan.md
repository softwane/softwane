## 三、Shutdown 流程的死锁陷阱

这是退出问题里**最容易踩坑**的一处，必须先讲清楚。

### 3.1 朴素方案 → 死锁

```rust
RunEvent::ExitRequested { api, .. } => {
    let handle = app.state::<EngineHandle>();
    handle.tx.blocking_send(EngineEvent::Shutdown).ok();
    handle.join.lock().take().map(|h| h.join());  // ← main thread 阻塞在这里
}
```

Engine thread 收到 `Shutdown` 后想关闭 renderer：

```rust
fn shutdown_renderer(&mut self, app: &AppHandle) {
    app.run_on_main_thread(move || {
        // MagSetFullscreenColorEffect(identity);
        // MagUninitialize();
    }).ok();
}
```

`run_on_main_thread` 是把闭包**投递到 main thread 的事件循环里执行**，本身不会阻塞。但 main thread 此刻正在 `join()` 上**同步阻塞**，事件循环没人 pump，闭包永远执行不到。Engine thread 等 renderer 关闭完成，main thread 等 engine thread 退出 —— **死锁**。

### 3.2 正确方案：`prevent_exit` + 后台 cleanup

```rust
.run(|app, event| match event {
    RunEvent::ExitRequested { api, code, .. } => {
        // 用静态/Arc<AtomicBool> 守门，避免 cleanup 完后再次进入这里时无限递归
        if !CLEANUP_DONE.load(Ordering::Acquire) {
            api.prevent_exit();  // ← 关键：放手 main thread，让事件循环继续 pump
            let app2 = app.clone();
            thread::spawn(move || {
                let handle = app2.state::<EngineHandle>();
                let _ = handle.tx.blocking_send(EngineEvent::Shutdown);
                if let Some(jh) = handle.join.lock().unwrap().take() {
                    let _ = jh.join();
                }
                CLEANUP_DONE.store(true, Ordering::Release);
                app2.exit(code.unwrap_or(0));  // ← 再次触发 ExitRequested，这次走 if 外
            });
        }
    }
    _ => {}
});
```

关键点：

- `prevent_exit()` 让 main thread 立即从 ExitRequested 处理中返回，回到事件循环
- Cleanup 在新线程里阻塞地等 engine join；engine 此时调用 `run_on_main_thread` 能正常被 main thread 执行
- Cleanup 完后 `app.exit(...)` 再触发一次 ExitRequested，靠 atomic flag 走过 if 体，本次正常退出

### 3.3 Engine thread 内部的 shutdown 顺序

收到 `EngineEvent::Shutdown` 后：

```rust
fn shutdown(&mut self, app: &AppHandle) {
    // 1. 状态机清理（如果未来有什么需要清理的；目前是 no-op）
    
    // 2. 配置保存（同步 OR 后台 + 等待）
    let save_join = if self.config_dirty {
        let snapshot = self.configs.clone();
        Some(thread::spawn(move || snapshot.save_to_disk()))
    } else {
        None
    };
    
    // 3. Renderer 关闭（dispatch 到 main thread，同步等待）
    //    注意 try_shutdown 内部要"投递闭包 + 等闭包跑完"
    self.renderer.try_shutdown(app);
    
    // 4. 等保存完成
    if let Some(h) = save_join {
        let _ = h.join();
    }
}
// 函数返回后 engine 主循环 break，线程退出，cleanup 线程的 join() 解除阻塞
```

这正是你说的"独占可变借用，与配置保存一起异步执行后 await"——因为 Engine 独占 `Renderer` 和 `AppConfig`，可以自由 `&mut` 它们；保存在另一个 worker thread 上跑（snapshot 后），主关闭路径并行做 renderer 关闭，最后 `join()` 等齐。

"save 未完成如何安全退出"在这个结构里**自动消失了**：cleanup 线程必然会等到 `engine_thread.join()` 返回，而 engine thread 必然会等到 `save_join.join()` 返回。整条链是阻塞串联的，到最后才 `app.exit()`。**只要 engine thread 的 shutdown 函数同步等齐所有清理，save 就一定完成**。

### 3.4 Renderer 的 `try_shutdown` 实现要点

要做到"投递 + 等待"，需要在 dispatch 时自带一个 oneshot：

```rust
fn try_shutdown(&mut self, app: &AppHandle) {
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel::<()>(0);
    let initialized = Arc::clone(&self.magnification_initialized);
    
    let dispatch_result = app.run_on_main_thread(move || {
        if initialized.load(Ordering::Acquire) {
            // 恢复 identity 效果
            let identity = MAGCOLOREFFECT { transform: IDENTITY_5x5_F32 };
            unsafe { MagSetFullscreenColorEffect(&identity) };
            unsafe { MagUninitialize() };
            initialized.store(false, Ordering::Release);
        }
        let _ = done_tx.send(());
    });
    
    if dispatch_result.is_ok() {
        // 设个超时避免 main thread 异常时永久阻塞
        let _ = done_rx.recv_timeout(Duration::from_secs(3));
    }
}
```

注意这里 `try_shutdown` **被 engine thread 调用**，不是 main thread。run_on_main_thread 会跨线程把闭包送过去，oneshot 跨线程传完成信号。

---

## 四、Engine 主循环里的 shutdown 检测

`EngineEvent::Shutdown` 的位置选项：

```rust
loop {
    let frame_events = self.collect_events();  // 在这里 drain 所有事件
    
    if frame_events.shutdown_requested {
        self.shutdown(&self.app);
        break;
    }
    
    self.timer.tick(...);
    self.channels.tick(...);
    self.renderer.render(...);
    self.maybe_persist(...);
    self.sleep(...);
}
```

`collect_events` 把 `EngineEvent::Shutdown` 翻译成 `frame_events.shutdown_requested = true`。在 tick 之前判断、立即跳出，避免最后一帧执行了一半被打断。

不要把"break loop"塞进 collect_events 内部（让控制流更显式）。

---

## 五、关于 Drop

你说得对，**不要走 Drop**。除了 AppHandle 不可得这一条，还有几个理由：

1. **Drop 顺序不可控**：如果 Engine 拥有 `Renderer`、`AppConfig`、`SensoryChannels`，Rust 默认按声明顺序 drop。但你想要的是"先 save，再关 renderer"，drop 就反过来了（声明顺序 ≠ 业务顺序）。
2. **Drop 不能 await**，也不能返回错误，意味着任何错误处理都得 `let _ = ...`。
3. **panic unwinding 时 Drop 仍会运行**，意味着 panic 路径上可能调到 FFI，未定义行为。
4. **没法等 worker thread**：Drop 内 `join()` 是同步的，但如果在 main thread Drop 链里调，会挂住整个进程退出。

保留 Drop 的理由就只剩"调试用"——可以加一个：

```rust
impl Drop for EngineHandle {
    fn drop(&mut self) {
        if self.join.lock().unwrap().is_some() {
            tracing::error!("EngineHandle dropped without explicit shutdown!");
        }
    }
}
```

作为防御性日志，提醒未来如果有人忘记走 ExitRequested 钩子。

---

## 六、整体 Shutdown 流程图

```
                                                        
 用户操作                                                
   │                                                    
   ├─ Tray "Quit"  ────► app.exit(0)                    
   ├─ 全局快捷键   ────► app.exit(0)                    
   └─ OS 信号      ────► tauri 内部触发                 
                          │                              
                          ▼                              
              ┌───────────────────────┐                  
              │ RunEvent::ExitRequested│   on main thread
              │  CLEANUP_DONE = false ?│                  
              │     ├─ true: 让 tauri 正常退出           
              │     └─ false:                           
              │         api.prevent_exit()              
              │         spawn cleanup_thread            
              └────┬───────────────────┘                  
                   │                                      
                   │ main thread 立即返回事件循环         
                   │                                      
                   ▼                                      
              ┌──────────────────┐                       
              │ cleanup_thread   │                       
              │ ──────────────── │                       
              │ tx.send(Shutdown)│   ──────────────►     
              │ engine_join()    │   阻塞等              
              │                  │                       
              └──────────────────┘                       
                   ▲                                      
                   │                                      
              ┌────┴───────────────┐                     
              │ engine_thread      │                     
              │ ───────────────────│                     
              │ collect: Shutdown! │                     
              │ shutdown(&app):    │                     
              │  ├─ spawn save_th  │                     
              │  ├─ renderer       │                     
              │  │  .try_shutdown  │                     
              │  │  ├─ run_on_main │ ──────────────►    
              │  │  └─ recv done   │  ◄────────────      
              │  └─ save_th.join() │                     
              │ break loop         │                     
              └────────────────────┘                     
                   │                                      
                   │ engine thread 退出                  
                   ▼                                      
              cleanup_thread:                            
                CLEANUP_DONE = true                      
                app.exit(0) ───► RunEvent::ExitRequested 
                                  这次走 true 分支       
                                  tauri 正常退出
```

整个流程里，**main thread 一次都没被同步阻塞过超过一个 ExitRequested 处理时长**，事件循环始终在 pump，所以 `run_on_main_thread` 的闭包一定能跑。

---

## 八、总结：你的方案

- ✅ 不走 Drop（保留 debug 日志型 Drop）
- ✅ `RunEvent::ExitRequested` 处理：`prevent_exit` + spawn cleanup_thread + atomic flag 防递归
- ✅ Cleanup_thread 内：发 Shutdown event + join engine_thread + atomic 翻位 + 再次 `app.exit()`
- ✅ Engine thread 独占 SensoryChannels/Renderer/AppConfig
- ✅ Manage 只放 `EngineHandle { tx, join_handle }`
- ✅ Engine 收到 Shutdown：spawn save_worker + renderer.try_shutdown + join save_worker → break loop
- ✅ Renderer.try_shutdown：oneshot + run_on_main_thread + 超时

整套结构没有任何共享可变 state、没有死锁、save 完成性自然保证、复杂度集中在一个文件（engine 退出路径）。

---

## 九、悬置项

**Panic 路径**：engine thread panic 后如何让屏幕恢复 identity，这个跨 panic_hook + FFI 主线程亲和性 + Renderer 状态归属，是个独立问题，可以等正常 shutdown 跑通后再处理。先在 TODO 列表里挂着。

下轮你说"开始 Batch 1"我就把这套结构落到代码里。在那之前请确认上面第七节的 1~5 项你倾向哪种。

# Part A · 项目架构

## A.0 设计原则

1. **Engine-first**：先定主循环骨架，子系统接口由主循环反推
2. **单线程独占 + 消息总线**：Engine thread 独占所有 mutable state，外部只通过 mpsc 推命令
3. **Push 优先于 Pull**：状态查询走事件 emit + 前端缓存，避免共享可变 state
4. **显式生命周期**：不依赖 Drop，关键资源走显式 `shutdown(&AppHandle)` 方法

---

## A.1 三段式生命周期

```
┌── init phase ────────────────────────────────────┐
│  Tauri Builder::setup(|app| {                    │
│    1. 读持久化配置 (AppConfig::load_or_default)  │
│    2. 构造 mpsc::channel(256) → (event_tx, rx)   │
│    3. 构造 Engine（独占 channels/renderer/config）│
│    4. spawn engine thread → JoinHandle           │
│    5. app.manage(EngineHandle { tx, join })      │
│    6. tray::setup(app, event_tx.clone())         │
│    7. global_shortcut::setup(app, event_tx)      │
│  })                                              │
└──────────────────────────────────────────────────┘
                       │
                       ▼
┌── main loop phase (engine thread) ───────────────┐
│  loop {                                          │
│    now      = Instant::now()                     │
│    dt_ms    = (now - last_frame).as_millis()     │
│    fe       = collect_events()                   │
│      ├─ RendererEvent → observability.log()      │
│      └─ Shutdown      → fe.shutdown_requested    │
│    if fe.shutdown_requested { do_shutdown; break}│
│                                                  │
│    timer.tick(dt_ms, &mut fe, &cfg)              │
│    channels.tick(state, &mut fe)                 │
│    if fe.switch_changed:                         │
│        renderer.switch_subrenderer_states(...)   │
│    renderer.render(logic_frame, &app)            │
│    broadcast_progress(now, state)                │
│    maybe_persist(now, &mut fe, &cfg)             │
│                                                  │
│    last_frame = now                              │
│    sleep(state.recommended_tick_interval()       │
│          - now.elapsed())                        │
│  }                                               │
└──────────────────────────────────────────────────┘
                       │
                       ▼
┌── shutdown phase ────────────────────────────────┐
│  ExitRequested 钩子 → prevent_exit + cleanup_thread│
│  cleanup_thread:                                 │
│    tx.send(Shutdown) → engine.join()             │
│    engine 内: spawn save_worker || renderer.try_shutdown│
│              join save_worker                     │
│    CLEANUP_DONE = true; app.exit(code)           │
└──────────────────────────────────────────────────┘
```

## A.9 进度同步（主窗口 + Tray 双 sink）

```rust
struct ProgressSinks {
    frontend: Option<tauri::ipc::Channel<ProgressPayload>>,  // 前端 register 后填入
    last_tray_update: Instant,
}

const TRAY_THROTTLE: Duration = Duration::from_secs(1);

fn broadcast_progress(&mut self, now: Instant, state: &TimerState) {
    let payload = ProgressPayload::from(state);
    
    if let Some(ch) = &self.progress_sinks.frontend {
        let _ = ch.send(payload.clone());                    // 每帧
    }
    
    if now - self.progress_sinks.last_tray_update >= TRAY_THROTTLE {
        tray::update_title(&self.app, &payload);             // 1s 节流
        self.progress_sinks.last_tray_update = now;
    }
}
```

- 前端通过 `register_progress_channel(channel)` command 注册
- WebView 重连时再 register 一次，覆盖旧 channel
- Phase 切换走 `app.emit("phase_changed", ...)` 而非 channel（低频离散事件）

---

## A.10 Shutdown 设计

### 入口

| 来源                            | 触发方式                     |
| ------------------------------- | ---------------------------- |
| Tray "Quit" 菜单                | 回调内 `app.exit(0)`         |
| OS 信号 (Win 关机 / SIGINT)     | Tauri 内部触发 ExitRequested |
| 全局快捷键（未来）              | 回调内 `app.exit(0)`         |
| **窗口关闭按钮 / WebView 关闭** | **不触发**（按已确认语义）   |

### 主线程钩子

```rust
static CLEANUP_DONE: AtomicBool = AtomicBool::new(false);

app.run(|app, event| match event {
    RunEvent::ExitRequested { api, code, .. } => {
        if !CLEANUP_DONE.load(Ordering::Acquire) {
            api.prevent_exit();
            let app2 = app.clone();
            let exit_code = code.unwrap_or(0);
            std::thread::spawn(move || run_cleanup(app2, exit_code));
        }
    }
    _ => {}
});

fn run_cleanup(app: AppHandle, exit_code: i32) {
    let handle = app.state::<EngineHandle>();
    let _ = handle.tx.blocking_send(EngineEvent::Shutdown);
    if let Some(jh) = handle.join.lock().unwrap().take() {
        let _ = jh.join();
    }
    CLEANUP_DONE.store(true, Ordering::Release);
    app.exit(exit_code);
});
```

### Engine 内 shutdown

```rust
fn do_shutdown(&mut self, app: &AppHandle) {
    let save_worker = if self.persistence.last_dirty_ts.is_some() {
        let snap = self.config.clone();
        Some(std::thread::spawn(move || snap.save_to_disk()))
    } else { None };

    self.renderer.try_shutdown(app);            // 同步等渲染恢复 + uninit

    if let Some(h) = save_worker {
        let _ = h.join();                       // 等保存落盘
    }
}
```

### 已解决性质

- ✅ 死锁规避：`prevent_exit` 让 main thread 继续 pump，`run_on_main_thread` 闭包能跑
- ✅ Save 完整性：cleanup_thread 必然等到 engine.join()，engine 必然等到 save.join()
- ✅ 资源所有权：Engine 独占 Renderer/Config，无 Mutex
- ✅ 重入防护：CLEANUP_DONE atomic flag
- ⚠️ Panic 路径：engine panic 时屏幕颜色卡住（Windows）。**已悬置**，作为 TODO

### Renderer.try_shutdown 范本

```rust
fn try_shutdown(&mut self, app: &AppHandle) {
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel::<()>(0);
    let initialized = Arc::clone(&self.color_transformer.magnification_initialized);
    
    let dispatch_result = app.run_on_main_thread(move || {
        if initialized.load(Ordering::Acquire) {
            let identity = MAGCOLOREFFECT { transform: IDENTITY_5X5_F32 };
            unsafe { MagSetFullscreenColorEffect(&identity) };
            unsafe { MagUninitialize() };
            initialized.store(false, Ordering::Release);
        }
        let _ = done_tx.send(());
    });
    
    if dispatch_result.is_ok() {
        let _ = done_rx.recv_timeout(Duration::from_secs(3));
    }
}
```

---

## A.11 EngineHandle (Manage 持有)

```rust
pub struct EngineHandle {
    pub tx: tokio::sync::mpsc::Sender<EngineEvent>,
    pub join: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
}

// Tauri command 范式
#[tauri::command]
fn start_session(handle: tauri::State<EngineHandle>, target_duration_ms: u64) -> Result<(), String> {
    handle.tx
        .blocking_send(EngineEvent::State(StateCommand::StartSession { target_duration_ms }))
        .map_err(|e| e.to_string())
}
```

注意：`tauri::command` 默认是 async 的；这里如果用 `blocking_send` 会在 tokio runtime 上 panic。需要改成：

- 把 command 标记为 sync（`#[tauri::command(async)]` 反过来），或
- 改用 `tx.send(...).await`（command 本身是 async fn）

实施时统一用 async + `await`。

## B6 · Shutdown

- [ ] `lib.rs` 内定义 `static CLEANUP_DONE: AtomicBool`
- [ ] `Builder::build()` + `app.run(|app, event| ...)` 模式（替换当前 `.run(generate_context!())`）
- [ ] `RunEvent::ExitRequested` 处理：`prevent_exit` + spawn cleanup_thread
- [ ] `run_cleanup` 函数：`tx.send(Shutdown)` → `engine.join()` → 翻 atomic → `app.exit()`
- [ ] `Engine::do_shutdown` 实现：spawn save_worker（条件性）+ `renderer.try_shutdown` + join save_worker
- [ ] `EngineHandle::Drop` 加 debug log（防御性提醒）
- [ ] 集成测试：tray Quit → 完整 cleanup → 进程退出码 0