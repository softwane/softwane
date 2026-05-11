//! `MacOSRendererDispatcher` — platform dispatcher for `target_os = "macos"`.
//!
//! Owns a single [`CoreGraphicsGammaRenderer`] sub-renderer and forwards
//! colour-temperature and brightness channel values to it.  Saturation is
//! discarded because Core Graphics gamma tables cannot de-saturate across
//! channels (TODO: explore accessibility API).

use std::sync::Arc;

use tauri::AppHandle;
use tokio::sync::mpsc::Sender;

use crate::channels::{ChannelSwitchStates, ChannelType, LogicFrame};
use crate::events::EngineEvent;

use super::mac_core_graphics_gamma_tweaker::CoreGraphicsGammaTweaker;

#[derive(Debug)]
pub struct MacOSRendererDispatcher {
    tx: Sender<EngineEvent>,
    gamma_tweaker: CoreGraphicsGammaTweaker,
}

impl MacOSRendererDispatcher {
    pub fn new(
        tx: Sender<EngineEvent>,
        states: ChannelSwitchStates,
        app: &AppHandle,
    ) -> Self {
        let mut this = Self {
            tx,
            gamma_tweaker: CoreGraphicsGammaTweaker::new(),
        };
        this.switch_renderer(states, app);
        this
    }

    pub fn dispatch(&mut self, logic_frame: Arc<LogicFrame>, app: &AppHandle) {
        // TODO: Try to support saturation via the macOS accessibility API.
        // Gamma tables cannot mix across channels, but a future
        // AXCustomizableColorFilter or similar may offer a path.
        let _saturation = logic_frame[ChannelType::Saturation];

        let color_temperature = logic_frame[ChannelType::ColorTemp];
        let brightness = logic_frame[ChannelType::Brightness];

        self.gamma_tweaker.render(
            color_temperature,
            brightness,
            app,
            self.tx.clone(),
        );
    }

    pub fn switch_renderer(
        &mut self,
        states: ChannelSwitchStates,
        app: &AppHandle,
    ) {
        let any_on =
            states[ChannelType::ColorTemp] || states[ChannelType::Brightness];
        if any_on {
            self.gamma_tweaker.startup(app, self.tx.clone());
        } else {
            self.gamma_tweaker.shutdown(app, self.tx.clone());
        }
    }

    pub fn shutdown(&mut self, app: &AppHandle) {
        self.gamma_tweaker.shutdown(app, self.tx.clone());
    }

    pub fn reset(&mut self, states: ChannelSwitchStates, app: &AppHandle) {
        self.shutdown(app);
        let any_on =
            states[ChannelType::ColorTemp] || states[ChannelType::Brightness];
        if any_on {
            self.gamma_tweaker.startup(app, self.tx.clone());
        }
    }
}
