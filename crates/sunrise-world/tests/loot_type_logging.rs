use std::sync::Mutex;
use sunrise_world::loot::types::Types;

struct Capture(Mutex<Vec<String>>);
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        self.0.lock().unwrap().push(record.args().to_string());
    }
    fn flush(&self) {}
}
static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));

#[test]
fn callback_refusals_warn_once_per_registry_type_and_data_generation() {
    log::set_logger(&CAPTURE).unwrap();
    log::set_max_level(log::LevelFilter::Warn);
    let generation = Types::default();
    for _ in 0..3 {
        generation.warn_once("test:broken", true, "function refused");
        generation
            .clone()
            .warn_once("test:broken", true, "function refused");
        generation.warn_once("test:broken", false, "condition refused");
    }
    Types::default().warn_once("test:broken", true, "new generation refused");
    assert_eq!(
        *CAPTURE.0.lock().unwrap(),
        [
            "loot function type test:broken: function refused",
            "loot condition type test:broken: condition refused",
            "loot function type test:broken: new generation refused",
        ]
    );
}
