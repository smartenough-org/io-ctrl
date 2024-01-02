
pub struct Board {
    pub hardware: Hardware,
    pub shared_resource: &'static SharedResource,
}


impl Board {
    pub fn init() -> Self {
    }

    pub fn start_tasks(&'static self, spawner: &Spawner) -> &Self {
        self.hardware.start_tasks(spawner);
        self
    }
}
