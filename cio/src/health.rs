use anyhow::Result;

#[derive(Debug, Default)]
pub struct SelfMemory {
    pub pid: i32,
    pub page_size: u64,
    pub stat_size: Option<u64>,
    pub stat_rss: Option<u64>,
    pub statm_size: Option<u64>,
    pub statm_rss: Option<u64>,
    pub statm_shared: Option<u64>,
    pub status_vmsize: Option<u64>,
    pub status_vmrss: Option<u64>,
    pub status_rssfile: Option<u64>,
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

#[derive(Debug)]
pub struct Health {
    pub memory: SelfMemory,
    pub processes: Vec<(i32, Result<String, procfs::ProcError>)>,
    pub tmp_size: Result<u64, fs_extra::error::Error>,
    pub github_cache_size: Result<u64, fs_extra::error::Error>,
}

pub fn get_health() -> Health {
    Health {
        memory: SelfMemory::new().expect("Failed to read memory during health check"),
        processes: procfs::process::all_processes()
            .expect("Failed to list processes")
            .filter_map(|res| {
                res.ok().and_then(|proc| {
                    let comm = proc.stat().map(|stat| stat.comm);

                    let is_webhooky = comm.map(|c| c.contains("webhooky")).unwrap_or(false);

                    if is_webhooky {
                        Some((proc.pid, proc.stat().map(|stat| stat.comm)))
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>(),
        tmp_size: fs_extra::dir::get_size("/tmp"),
        github_cache_size: fs_extra::dir::get_size("/tmp/.cache/github"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_health_runs() {
        pretty_env_logger::init();

        let _ = super::get_health();
    }

    #[test]
    fn test_self_memory_runs() {
        let mem = SelfMemory::new();
        assert!(mem.is_ok());
    }
}
