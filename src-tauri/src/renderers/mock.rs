//! Mock renderer dispatcher — logs every call via `debug!` but performs no
//! actual rendering.  Useful for testing the engine loop on platforms without
//! a real sub-renderer (e.g. Linux) and in unit tests.

use std::sync::Arc;

use tauri::AppHandle;
use tokio::sync::mpsc::Sender;

use crate::channels::{ChannelSwitchStates, LogicFrame};
use crate::events::{EngineEvent, RendererEvent};

// TODO: add test using mock

#[derive(Debug)]
pub struct MockRendererDispatcher {
    tx: Sender<EngineEvent>,
}

impl MockRendererDispatcher {
    pub fn new(
        tx: Sender<EngineEvent>,
        _states: ChannelSwitchStates,
        _app: &AppHandle,
    ) -> Self {
        tracing::debug!("MockRendererDispatcher::new");
        Self { tx }
    }

    pub fn dispatch(&mut self, _logic_frame: Arc<LogicFrame>, _app: &AppHandle) {
        tracing::debug!("MockRendererDispatcher::dispatch");
    }

    pub fn switch_renderer(
        &mut self,
        states: ChannelSwitchStates,
        _app: &AppHandle,
    ) {
        tracing::debug!(
            "MockRendererDispatcher::switch_renderer  states={:?}",
            states
        );
    }

    pub fn shutdown(&mut self, _app: &AppHandle) {
        tracing::debug!("MockRendererDispatcher::shutdown");
        let _ = self.tx.try_send(EngineEvent::Renderer(
            RendererEvent::ShutdownCompleted {
                renderer_name: "mock-dispatcher",
            },
        ));
    }

    pub fn reset(&mut self, _states: ChannelSwitchStates, _app: &AppHandle) {
        tracing::debug!("MockRendererDispatcher::reset");
    }
}
