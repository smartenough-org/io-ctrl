/// TODO Do I need this?
pub struct Shared {}

impl Default for Shared {
    fn default() -> Self {
        Shared::new()
    }
}

impl Shared {
    pub fn new() -> Self {
        Self {}
    }
}
