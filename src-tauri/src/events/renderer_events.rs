#[derive(Debug)]
pub enum RendererEvent {
    RenderUnappliedDueToUnchanged {
        renderer_name: &'static str,
    },
    /// The sub-renderer received a `render()` call before `startup()` completed.
    RenderUnappliedDueToNotStartupped {
        renderer_name: &'static str,
    },
    RenderSuccessful {
        renderer_name: &'static str,
    },
    RenderFailed {
        renderer_name: &'static str,
        error: String,
    },
    /// `startup()` completed successfully (MagInitialize returned non-zero).
    StartupCompleted {
        renderer_name: &'static str,
    },
    /// `startup()` failed (MagInitialize returned 0).
    StartupFailed {
        renderer_name: &'static str,
        error: String,
    },
    /// All main-thread closures queued by `shutdown()` have executed,
    /// and `uninit_api` has either called MagUninitialize or fallen back.
    /// The engine can safely proceed past the shutdown drain loop upon
    /// receiving this event.
    ShutdownCompleted {
        renderer_name: &'static str,
    },
}
