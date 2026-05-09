use super::*;

// TODO: add abnormal_shutdown which consume self and run in the main thread
// This means run() shoul return Self

impl Engine {
    pub(super) fn shutdown(&mut self) {
        tracing::info!("engine shutdown begin");

        // 1. Dispatch shutdown closures (non-blocking).
        self.renderers.shutdown(&self.app);
        let store_for_closure = self.store.clone();
        let save_handle = std::thread::spawn( move || {
            if let Err(e) = store_for_closure.save() {
                tracing::error!("failed to save store during shutdown: {e}");
            }
        });

        // 2. Drain events within the timeout, waiting for ShutdownCompleted.
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        let mut acked = false;

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

        // 3. Wait for persisting store to disk (synchronous, blocks until write completes).
        if let Err(e) = save_handle.join() {
            tracing::error!("failed to save store during shutdown: {e:?}.");
        }

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

            self.renderers.shutdown(&self.app);
            let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
            let mut acked = false;
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
            let _ = self.store.save();
            // tracing may not work in such cases
            let _ = std::io::stderr().write_fmt(
                format_args!("[Engine::drop] panic recovery cleanup done\n"),
            );
        })) {
            tracing::error!(
                "[Engine::drop] panic recovery shutting down fails:\n{:#?}\n",
                err
            );
            // tracing may not work in such cases
            let _ = std::io::stderr().write_fmt(
            format_args!(
                    "[Engine::drop] panic recovery shutting down fails:\n{:#?}\n",
                    err
                )
            );
        };
    }
}
