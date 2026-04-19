pub enum RendererEvent {
    RenderUnappliedDueToUnchanged {
        sub_renderer_name: &'static str,
    },
    RenderSuccessful {
        sub_renderer_name: &'static str,
    },
    RenderFailed {
        sub_renderer_name: &'static str,
        error: String,
    },
}