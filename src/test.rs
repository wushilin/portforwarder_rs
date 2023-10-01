pub mod backend;

use backend::{HostGroupTracker, HostGroup};
#[tokio::main]
async fn main() {
    let mut hgt = HostGroupTracker::new(10000);
    let mut target = HostGroup::new("some");
    target.add("192.168.44.105:80");
    target.add("192.168.44.160:80");
    target.add("www.google.com:80");
    target.add("www.google.com:8041");
    target.add("www.goofawef322gle.com:8041");
    hgt.add(target);

    let mut target1 = HostGroup::new("some1");
    target1.add("192.168.44.105:22");
    target1.add("192.168.44.160:22");
    target1.add("www.google.com:22");
    target1.add("www.youtube.com:22");
    target1.add("www.yoawefawg3234utube.com:22");
    hgt.add(target1);
    hgt.start_checker();

    loop {
        let selected = hgt.select("some");
        println!("Selected `some` -> {selected:?}");
        let selected = hgt.select("some1");
        println!("Selected `some1` -> {selected:?}");
        std::thread::sleep(std::time::Duration::from_millis(1000));
    }
    
}