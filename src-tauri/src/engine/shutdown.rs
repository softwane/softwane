use std::{
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind},
    thread,
};

use tokio::sync::mpsc::error::TryRecvError;
use super::*;

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub fn is_main_thread() -> bool {
    thread::current().name() == Some("main")
}

impl Engine {
    pub(super) fn shutdown(&mut self) {
        tracing::info!("engine shutdown begin");

        // 1. Dispatch shutdown closures (non-blocking).
        self.renderers.shutdown(&self.app);

        // 2. Drain events within the timeout, waiting for ShutdownCompleted.
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        let mut acked = false;

        // FIXME: this only works when there is only one renderer.
        // It's related to how dispatcher communicate with engine.
        while !acked && Instant::now() < deadline {
            match self.event_rx.try_recv() {
                Ok(EngineEvent::Renderer(RendererEvent::ShutdownCompleted { renderer_name })) => {
                    tracing::info!(renderer_name, "renderer shutdown acked");
                    acked = true;
                }
                Ok(EngineEvent::Renderer(other)) => {
                    tracing::debug!(?other, "drained during shutdown");
                }
                Ok(_) => {
                    // Inbound commands during shutdown are discarded.
                }
                Err(TryRecvError::Empty) => std::thread::sleep(SHUTDOWN_POLL_INTERVAL),
                Err(TryRecvError::Disconnected) => break,
            }
        }

        if !acked {
            tracing::warn!(
                "renderer shutdown timed out after {:?}.",
                SHUTDOWN_TIMEOUT
            );
        }

        self.cleaned_up = true;
        tracing::info!("engine shutdown complete");
    }

    pub fn shutdown_on_main_thread(&mut self) {
        if self.cleaned_up {
            return;
        }

        tracing::info!("engine shutdown begin");

        self.renderers.shutdown_on_main_thread(&self.app);

        self.cleaned_up = true;
        tracing::info!("engine shutdown complete");
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if self.cleaned_up {
            return;
        }

        // Abnormal path: engine was dropped without shutdown (panic unwinding).
        // catch_unwind wraps all cleanup to prevent double-panic → abort.
        if let Err(err) = catch_unwind(AssertUnwindSafe(|| {
            // Avoid potential new panic info overlaying the old one.
            // See the hook in lib.rs
            let _prev = std::panic::take_hook();

            if is_main_thread() {
                self.shutdown_on_main_thread();
            } else {
                self.shutdown();
            }

            tracing::info!("[Engine::drop] panic recovery cleanup done");
            // tracing may not work in such cases
            let _ = std::io::stderr().write_fmt(
                format_args!("[Engine::drop] panic recovery cleanup done\n"),
            );
        })) {
            tracing::error!(
                "[Engine::drop] panic recovery shutting down fails:\n{:#?}\n",
                err,
            );
            // tracing may not work in such cases
            let _ = std::io::stderr().write_fmt(
                format_args!(
                    "[Engine::drop] panic recovery shutting down fails:\n{:#?}\n",
                    err,
                )
            );
        };
    }
}
