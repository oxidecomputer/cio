use cio_api::health::get_health;

pub fn scheduler_health_check() {
    log::info!("Scheduler heartbeat");

    let health = get_health();

    log::info!("Report health {:?}", health);
}

pub fn report_health(label: &str) {
    let health = get_health();
    log::info!("[{}] Health {:?}", label, health);
}
