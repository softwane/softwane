//! `WindowsRendererDispatcher` — platform dispatcher for `target_os = "windows"`.
//!
//! Owns a single [`WinMagAPIColorTransformer`] sub-renderer and forwards each
//! frame's channel values to it.

use std::sync::Arc;

use tauri::AppHandle;
use tokio::sync::mpsc::Sender;

use crate::channels::{ChannelSwitchStates, ChannelType, LogicFrame};
use crate::events::EngineEvent;

use super::win_magapi_color_transformer::WinMagAPIColorTransformer;

#[derive(Debug)]
pub struct WindowsRendererDispatcher {
    tx: Sender<EngineEvent>,
    color_transformer: WinMagAPIColorTransformer,
}

impl WindowsRendererDispatcher {
    pub fn new(
        tx: Sender<EngineEvent>,
        states: ChannelSwitchStates,
        app: &AppHandle,
    ) -> Self {
        let mut this = Self {
            tx,
            color_transformer: WinMagAPIColorTransformer::new(),
        };
        this.switch_renderer(states, app);
        this
    }

    // TODO:track color transformer's state, if it doesn't excuted cuz the main thread is stuck, send event
    pub fn dispatch(&mut self, logic_frame: Arc<LogicFrame>, app: &AppHandle) {
        let saturation = logic_frame[ChannelType::Saturation];
        let color_temperature = logic_frame[ChannelType::ColorTemp];
        let brightness = logic_frame[ChannelType::Brightness];
        self.color_transformer.render(
            saturation,
            color_temperature,
            brightness,
            app,
            self.tx.clone(),
        );
    }

    pub fn switch_renderer(&mut self, states: ChannelSwitchStates, app: &AppHandle) {
        let any_on = states[ChannelType::Saturation]
            || states[ChannelType::ColorTemp]
            || states[ChannelType::Brightness];
        if any_on {
            self.color_transformer.startup(app, self.tx.clone());
        } else {
            self.color_transformer.shutdown(app, self.tx.clone());
        }
    }

    pub fn shutdown(&mut self, app: &AppHandle) {
        self.color_transformer.shutdown(app, self.tx.clone());
    }

    pub fn reset(&mut self, states: ChannelSwitchStates, app: &AppHandle) {
        self.shutdown(app);
        let any_on = states[ChannelType::Saturation]
            || states[ChannelType::ColorTemp]
            || states[ChannelType::Brightness];
        if any_on {
            self.color_transformer.startup(app, self.tx.clone());
        }
    }
}
