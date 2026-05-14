//! `MacOSRendererDispatcher` — platform dispatcher for `target_os = "macos"`.
//!
//! Owns a single [`CoreGraphicsGammaRenderer`] sub-renderer and forwards
//! saturation, colour-temperature, and brightness channel values to it.

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

impl ChannelType {
    pub const fn conflicts(&self) -> &'static [ChannelType] {
        match self {
            Self::Brightness => &[Self::Saturation],
            Self::ColorTemp => &[Self::Saturation],
            Self::Saturation => &[Self::Brightness, Self::ColorTemp],
        }
    }
}

#[derive(Debug)]
pub struct MacOSRendererDispatcher {
    tx: Sender<EngineEvent>,
    saturation_filter: MacColorSyncSaturationFilter,
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
            saturation_filter: MacColorSyncSaturationFilter::new(),
            gamma_tweaker: CoreGraphicsGammaTweaker::new(),
        };
        this.switch_renderer(states, app);
        this
    }

    pub fn dispatch(&mut self, logic_frame: Arc<LogicFrame>, app: &AppHandle) {
        let saturation = logic_frame[ChannelType::Saturation];
        let color_temperature = logic_frame[ChannelType::ColorTemp];
        let brightness = logic_frame[ChannelType::Brightness];

        self.gamma_tweaker.render(
            color_temperature,
            brightness,
            app,
            self.tx.clone(),
        );
        self.saturation_filter
            .render(saturation, app, self.tx.clone());
    }

    pub fn switch_renderer(
        &mut self,
        states: ChannelSwitchStates,
        app: &AppHandle,
    ) {
        if states[ChannelType::ColorTemp] || states[ChannelType::Brightness] {
            self.gamma_tweaker.startup(app, self.tx.clone());
        } else {
            self.gamma_tweaker.shutdown(app, self.tx.clone());
        }

        if states[ChannelType::Saturation] {
            self.saturation_filter.startup(app, self.tx.clone());
        } else {
            self.saturation_filter.shutdown(app, self.tx.clone());
        }
    }

    pub fn shutdown(&mut self, app: &AppHandle) {
        self.gamma_tweaker.shutdown(app, self.tx.clone());
        self.saturation_filter.shutdown(app, self.tx.clone());
    }

    pub fn reset(&mut self, states: ChannelSwitchStates, app: &AppHandle) {
        self.shutdown(app);
        if states[ChannelType::ColorTemp] || states[ChannelType::Brightness] {
            self.gamma_tweaker.startup(app, self.tx.clone());
        }
        if states[ChannelType::Saturation] {
            self.saturation_filter.startup(app, self.tx.clone());
        }
    }
}
