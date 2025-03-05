#![no_std]
#![no_main]

#[cfg(test)]
fn setup_log() {
    // Doing it early.
    rtt_target::rtt_init_defmt!();
}

#[cfg(test)]
#[embedded_test::tests(setup=crate::setup_log())]
mod tests {
    #[init]
    async fn init() {}

    #[test]
    fn shutters() {
        use io_ctrl::buttonsmash::shutters;
        shutters::tests::it_builds();
    }

    #[test]
    fn bindings() {
        use io_ctrl::buttonsmash::bindings;
        bindings::tests::it_adds_and_finds();
    }
}
