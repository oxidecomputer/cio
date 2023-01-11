pub fn scheduler_health_check() {
    log::info!("Scheduler heartbeat");

    let cache_size = fs_extra::dir::get_size("/tmp/.cache/github");
    log::info!("GitHub cache size {:?}", cache_size);
}

#[cfg(test)]
mod tests {
    use super::scheduler_health_check;

    #[test]
    fn test_scheduler_health_check_runs() {
        pretty_env_logger::init();

        let _ = super::scheduler_health_check();
    }
}