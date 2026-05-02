//! `MacOSRendererDispatcher` — no-op platform dispatcher for `target_os = "macos"`.
//!
//! All methods are empty except [`shutdown`], which immediately emits
//! [`ShutdownCompleted`] so that the Engine's shutdown drain loop works
//! the same way on every platform.

use std::sync::Arc;

use tauri::AppHandle;
use tokio::sync::mpsc::Sender;

use crate::channels::{ChannelSwitchStates, LogicFrame};
use crate::events::{EngineEvent, RendererEvent};

pub struct MacOSRendererDispatcher {
    tx: Sender<EngineEvent>,
}

impl MacOSRendererDispatcher {
    pub fn new(tx: Sender<EngineEvent>) -> Self {
        Self { tx }
    }

    pub fn dispatch(&mut self, _logic_frame: Arc<LogicFrame>, _app: &AppHandle) {}

    pub fn switch_renderer(
        &mut self,
        _states: ChannelSwitchStates,
        _app: &AppHandle,
    ) {
    }

    pub fn shutdown(&mut self, _app: &AppHandle) {
        let _ = self.tx.try_send(EngineEvent::Renderer(
            RendererEvent::ShutdownCompleted {
                renderer_name: "macos-dispatcher",
            },
        ));
    }

    pub fn reset(&mut self, _states: ChannelSwitchStates, _app: &AppHandle) {}
}
