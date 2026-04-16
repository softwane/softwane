pub struct FrameFlags {
    pub just_transited: bool,
}

impl Default for FrameFlags {
    fn default() -> Self {
        Self { just_transited: false }
    }
}