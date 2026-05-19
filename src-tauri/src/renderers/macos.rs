//! `MacOSRendererDispatcher` — platform dispatcher for `target_os = "macos"`.
//!
//! Forwards saturation, colour-temperature, and brightness channel values
//! to the unified [`MacColorSyncSaturationFilter`] which modifies both the
//! ICC profile colorant matrix (rXYZ/gXYZ/bXYZ) and the vcgt tag in a single
//! profile via `ColorSyncDeviceSetCustomProfiles`.

use std::sync::Arc;

use tauri::AppHandle;
use tokio::sync::mpsc::Sender;

use crate::{
    channels::{ChannelSwitchStates, ChannelType, LogicFrame},
    engine::EngineEvent,
};
use super::{
    mac_colorsync_saturation_filter::MacColorSyncSaturationFilter,
    mac_core_graphics_gamma_tweaker::CoreGraphicsGammaTweaker,
};

#[derive(Debug)]
pub struct MacOSRendererDispatcher {
    tx: Sender<EngineEvent>,
    renderer: MacColorSyncSaturationFilter,
}

impl MacOSRendererDispatcher {
    pub fn new(
        tx: Sender<EngineEvent>,
        states: ChannelSwitchStates,
        app: &AppHandle,
    ) -> Self {
        let mut this = Self {
            tx,
            renderer: MacColorSyncSaturationFilter::new(),
        };
        this.switch_renderer(states, app);
        this
    }

    pub fn dispatch(&mut self, logic_frame: Arc<LogicFrame>, app: &AppHandle) {
        let color_temperature = logic_frame[ChannelType::ColorTemp];
        let saturation = logic_frame[ChannelType::Saturation];
        let brightness = logic_frame[ChannelType::Brightness];

        self.renderer.render(
            color_temperature,
            saturation,
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
        let any_on = states[ChannelType::Saturation]
            || states[ChannelType::ColorTemp]
            || states[ChannelType::Brightness];
        if any_on {
            self.renderer.startup(app, self.tx.clone());
        } else {
            self.renderer.shutdown(app, self.tx.clone());
        }
    }

    pub fn shutdown(&mut self, app: &AppHandle) {
        self.renderer.shutdown(app, self.tx.clone());
    }

    pub fn shutdown_on_main_thread(&mut self, app: &AppHandle) {
        self.renderer.shutdown(app, self.tx.clone());
    }

    pub fn reset(&mut self, states: ChannelSwitchStates, app: &AppHandle) {
        self.shutdown(app);
        if states[ChannelType::Saturation]
            || states[ChannelType::ColorTemp]
            || states[ChannelType::Brightness]
        {
            self.renderer.startup(app, self.tx.clone());
        }
    }

    pub fn prepare_send(&mut self) {
        self.renderer.prepare_send();
    }
}
