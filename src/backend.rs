use std::collections::{HashMap, HashSet};
use std::time::{Instant, Duration};
use std::sync::{Arc, RwLock};
use std::error::Error;
use crate::resolve::ResolveConfig;

/// Host checker checks a bunch of hosts and report whether the target is reachable. 
/// The timeout is always 3 seconds as it seems to be reasonable!
#[derive(Debug)]
pub struct HostChecker {
    /// Connect timeout
    timeout: Duration,
    resolver: ResolveConfig,
}

/// Host checker check all hosts given at the same time, using the timeout specified.
/// For each host, the result is guaranteed to be provided no matter what error happened.
impl HostChecker {
    pub async fn check(&self, hosts_v:Vec<String>) -> HashMap<String, bool> {
        let mut hosts = HashSet::<String>::new();
        hosts.extend(hosts_v);
        let mut result_map = HashMap::<String,bool>::new();
        let mut jhs = Vec::<tokio::task::JoinHandle<(String, bool)>>::new();
        for i in &hosts {
            let local_hp = i.clone();
            let timeout = self.timeout;
            let resolver = self.resolver.clone();
            let jh = tokio::spawn(async move {
                let result = Self::check_one(resolver,&local_hp, timeout).await;
                if result.is_err() {
                    return (local_hp, false)
                } else {
                    return (local_hp, true)
                }
            });
            jhs.push(jh);
        }

        for jh in jhs {
            let (hp, result) = jh.await.unwrap();
            result_map.insert(hp, result);
        }
        return result_map;
    }


    async fn check_one(resolver:ResolveConfig, host_str:&String, timeout:Duration) -> Result<(), Box<dyn Error>>{
        let resolved = resolver.resolve(host_str);
        let _ = tokio::time::timeout(
            timeout,
            tokio::net::TcpStream::connect(&resolved)
        ).await??;
        return Ok(());
    }
}

#[derive(Debug, Clone)]
pub struct HostGroup {
    name:String,
    up: Vec<(String, Instant)>, // the up hosts under this group
    down: Vec<(String, Instant)>, //
}

impl HostGroup {
    pub fn new(name:&str)-> HostGroup {
        return HostGroup {
            name:name.to_string(),
            up: Vec::new(),
            down: Vec::new(),
        }
    }

    pub fn add(&mut self, host:&str) {
        self.up.push((host.to_string(), Instant::now()));
    }

    pub fn get_all(&self) -> Vec<String> {
        let mut result = Vec::<String>::new();
        for (hp, _) in &self.up {
            result.push(hp.clone())
        }
        for(hp, _) in &self.down {
            result.push(hp.clone())
        }
        return result;
    }

    fn is_up(map:&HashMap<String, bool>, lookup:&String) -> bool {
        let tmp = map.get(lookup);
        if tmp.is_none() {
            return false;
        }
        return *tmp.unwrap();
    }
    pub fn update(&mut self, map:&HashMap<String, bool>) {
        let now = Instant::now();
        let up = &mut self.up;
        let down = &mut self.down;
        let mut up_to_down = Vec::<String>::new();
        let mut down_to_up = Vec::<String>::new();
        for hg in up {
            if !Self::is_up(map, &hg.0) {
                up_to_down.push(hg.0.clone());
            }
        }

        for hg in down {
            if Self::is_up(map, &hg.0) {
                down_to_up.push(hg.0.clone());
            }
        }

        let name = &self.name;
        for next in &up_to_down {
            Self::remove(&mut self.up, &next);
            self.down.push((next.clone(), now));
            println!("{name} {next:?} is down");
        }
        for next in &down_to_up {
            Self::remove(&mut self.down, &next);
            self.up.push((next.clone(), now));
            println!("{name} {next:?} is up");
        }
    
        if up_to_down.len() > 0 || down_to_up.len() > 0 {
            let up_count = self.up.len();
            let down_count = self.down.len();
            println!("{name} up {up_count} down {down_count}")
        }
    }

    fn remove(from:&mut Vec<(String, Instant)>, what:&String) {
        let idx = Self::index_of(from, what);
        if idx.is_some() {
            from.remove(idx.unwrap());
        }
    }

    fn index_of(from:&mut Vec<(String, Instant)>, find:&String) -> Option<usize> {
        for (idx, what) in from.iter().enumerate() {
            let what = &what.0;
            if what == find {
                return Some(idx);
            }
        }
        return None;
    }
}


#[derive(Debug)]
pub struct HostGroupTracker {
    host_groups: Arc<RwLock<HashMap<String, RwLock<HostGroup>>>>,
    host_checker: Arc<HostChecker>,
}

impl HostGroupTracker {
    pub fn new(timeout: u64, resolver:ResolveConfig) -> HostGroupTracker {
        let mut actual_timeout = timeout;
        if actual_timeout < 100 {
            actual_timeout = 100;
        }
        return HostGroupTracker { 
            host_groups: Arc::new(RwLock::new(HashMap::new())),
            host_checker: Arc::new(HostChecker {timeout: Duration::from_millis(actual_timeout), resolver}),
        }
    }

    pub fn add(&mut self, target:HostGroup) {
        self.host_groups.write().unwrap().insert(target.name.clone(), RwLock::new(target));
    }

    pub fn start_checker(&self) {
        let tracker_local = Arc::clone(&self.host_groups);
        let host_checker = Arc::clone(&self.host_checker);
        tokio::spawn(async move {
            loop {
                let mut hosts_to_check = HashMap::<String, HostGroup>::new();
                {
                    let w = tracker_local.read().unwrap();
                    let iter = w.iter();
                    for (name, group) in iter {
                        let group_clone = group.read().unwrap().clone();
                        let name_clone = name.clone();
                        hosts_to_check.insert(name_clone, group_clone);       
                    }
                }

                let mut all_hosts_only:Vec<String> = Vec::new();
                for (_, hg) in &hosts_to_check {
                    let mut local_host_list = hg.get_all();
                    all_hosts_only.append(&mut local_host_list);
                }
                let global_check_result = host_checker.check(all_hosts_only).await;
                for (name, _) in hosts_to_check {
                    let w = tracker_local.write().unwrap();
                    let mut wi = w.get(&name).unwrap().write().unwrap();
                    wi.update(&global_check_result);
                }

                tokio::time::sleep(Duration::from_millis(3000)).await
            }
        });
    }

    pub fn select(&self, name:&str) -> Option<String> {
        let result = self.host_groups.read();
        let result = result.unwrap();
        let result = result.get(name);
        if result.is_none() {
            return None;
        }
        let result = result.unwrap().read().unwrap();
        let up = &result.up;
        let down = &result.down;
        if up.len() > 0 {
            // select from up
            return Self::select_random(up);
        }
        if down.len() > 0 {
            return Self::select_random(down);
        }

        return None;
        // no up available, choosing from down!
    }

    fn select_random(what:&Vec<(String, Instant)>) -> Option<String>{
        let mut rand_usize:usize = rand::random();
        rand_usize = rand_usize % what.len();
        let (host_port, _) = what.get(rand_usize).unwrap();
        return Some(host_port.clone());
    }
}

