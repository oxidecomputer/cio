use anyhow::Result;

#[derive(Debug, Default)]
pub struct SelfMemory {
    pid: i32,
    page_size: u64,
    stat_size: Option<u64>,
    stat_rss: Option<u64>,
    statm_size: Option<u64>,
    statm_rss: Option<u64>,
    statm_shared: Option<u64>,
    status_vmsize: Option<u64>,
    status_vmrss: Option<u64>,
    status_rssfile: Option<u64>,
}

impl SelfMemory {
    pub fn new() -> Result<Self> {
        let mut instance = SelfMemory::default();

        let self_proc = procfs::process::Process::myself()?;
        let page_size = procfs::page_size()?;

        instance.pid = self_proc.pid;
        instance.page_size = page_size;

        if let Ok(stat) = self_proc.stat() {
            instance.stat_size = Some(stat.vsize);
            instance.stat_rss = Some(stat.rss * page_size);
        }

        if let Ok(statm) = self_proc.statm() {
            instance.statm_size = Some(statm.size * page_size);
            instance.statm_rss = Some(statm.resident * page_size);
            instance.statm_shared = Some(statm.shared * page_size);
        }

        if let Ok(status) = self_proc.status() {
            instance.status_vmsize = status.vmsize.map(|kb| kb * 1024);
            instance.status_vmrss = status.vmrss.map(|kb| kb * 1024);
            instance.status_rssfile = status
                .rssfile
                .and_then(|rssfile| status.rssshmem.map(|rssshmem| rssfile * 1024 + rssshmem * 1024))
        }

        Ok(instance)
    }
}

pub fn scheduler_health_check() {
    log::info!("Scheduler heartbeat");

    if let Ok(mem) = SelfMemory::new() {
        log::info!("Instance memory {:?}", mem);
    }

    let cache_size = fs_extra::dir::get_size("/tmp/.cache/github");
    log::info!("GitHub cache size {:?}", cache_size);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_health_check_runs() {
        pretty_env_logger::init();

        let _ = super::scheduler_health_check();
    }

    #[test]
    fn test_self_memory_runs() {
        let mem = SelfMemory::new();
        assert!(mem.is_ok());
    }
}
