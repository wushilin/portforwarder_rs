use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Instant, Duration};

pub struct ConnStats {
    id: usize,
    start: Instant,
    uploaded_bytes: Arc<AtomicUsize>,
    downloaded_bytes: Arc<AtomicUsize>,
    global_stats: Arc<GlobalStats>,
}

impl ConnStats {
    pub fn new(gstat:Arc<GlobalStats>) -> ConnStats {
        gstat.increase_conn_count();
        gstat.increase_active_conn_count();
        let result = ConnStats{
            id: gstat.gen_conn_id(),
            start: Instant::now(),
            uploaded_bytes: new_au(0),
            downloaded_bytes: new_au(0),
            global_stats: Arc::clone(&gstat),
        };
        return result;
    }

    pub fn elapsed(&self) -> Duration {
        return self.start.elapsed();
    }

    pub fn get_global_stats(&self) -> Arc<GlobalStats> {
        return Arc::clone(&self.global_stats);
    }

    pub fn id_str(&self) -> String {
        let id = self.id;
        return format!("conn {id}");
    }
    pub fn id(&self) -> usize {
        return self.id;
    }

    pub fn add_downloaded_bytes(&self, new: usize) -> usize {
        let result = self.downloaded_bytes.fetch_add(new, Ordering::SeqCst) + new;
        self.global_stats.add_downloaded_bytes(new);
        return result;
    }

    pub fn add_uploaded_bytes(&self, new: usize) -> usize {
        let result = self.uploaded_bytes.fetch_add(new, Ordering::SeqCst) + new;
        self.global_stats.add_uploaded_bytes(new);
        return result;
    }

    pub fn downloaded_bytes(&self) -> usize {
        return self.downloaded_bytes.load(Ordering::SeqCst);
    }

    pub fn uploaded_bytes(&self) -> usize {
        return self.uploaded_bytes.load(Ordering::SeqCst);
    }

}

impl Drop for ConnStats {
    fn drop(&mut self) {
        self.global_stats.decrease_active_conn_count();
    }
}
impl Clone for ConnStats {
    fn clone(&self) -> ConnStats {
        return ConnStats { 
            id: self.id,
            start: self.start, 
            uploaded_bytes: Arc::clone(&self.uploaded_bytes), 
            downloaded_bytes: Arc::clone(&self.downloaded_bytes),
            global_stats: Arc::clone(&self.global_stats),
        };
    }
}

pub struct GlobalStats {
    id_gen: Arc<AtomicUsize>,
    conn_count: Arc<AtomicUsize>,
    active_conn_count: Arc<AtomicUsize>,
    total_uploaded_bytes: Arc<AtomicUsize>,
    total_downloaded_bytes: Arc<AtomicUsize>,
}

impl Clone for GlobalStats {
    fn clone(&self) -> GlobalStats {
        return GlobalStats {
            id_gen: Arc::clone(&self.id_gen),
            conn_count: Arc::clone(&self.conn_count),
            active_conn_count: Arc::clone(&self.active_conn_count),
            total_downloaded_bytes: Arc::clone(&self.total_downloaded_bytes),
            total_uploaded_bytes: Arc::clone(&self.total_uploaded_bytes),
        };
    }
}
fn new_au(start: usize) -> Arc<AtomicUsize> {
    let au = AtomicUsize::new(start);
    return Arc::new(au);
}
impl GlobalStats {

    pub fn new() -> GlobalStats {
        return GlobalStats {
            id_gen: new_au(0),
            conn_count: new_au(0),
            active_conn_count: new_au(0),
            total_downloaded_bytes: new_au(0),
            total_uploaded_bytes: new_au(0),
        };
    }

    fn gen_conn_id(&self) -> usize {
        return self.id_gen.fetch_add(1, Ordering::SeqCst) + 1;
    }

    fn increase_conn_count(&self) -> usize {
        return self.conn_count.fetch_add(1, Ordering::SeqCst) + 1;
    }

    pub fn conn_count(&self)->usize {
        return self.conn_count.load(Ordering::SeqCst);
    }

    fn increase_active_conn_count(&self) -> usize {
        return self.active_conn_count.fetch_add(1, Ordering::SeqCst) + 1;
    }

    fn decrease_active_conn_count(&self) -> usize {
        return self.active_conn_count.fetch_sub(1, Ordering::SeqCst) - 1;
    }

    pub fn active_conn_count(&self) -> usize {
        return self.active_conn_count.load(Ordering::SeqCst);
    }

    fn add_downloaded_bytes(&self, new: usize) -> usize {
        return self.total_downloaded_bytes.fetch_add(new, Ordering::SeqCst) + new;
    }

    pub fn total_downloaded_bytes(&self) -> usize {
        return self.total_downloaded_bytes.load(Ordering::SeqCst);
    }

    fn add_uploaded_bytes(&self, new: usize) -> usize {
        return self.total_uploaded_bytes.fetch_add(new, Ordering::SeqCst) + new;
    }

    pub fn total_uploaded_bytes(&self) -> usize {
        return self.total_uploaded_bytes.load(Ordering::SeqCst);
    }
}
