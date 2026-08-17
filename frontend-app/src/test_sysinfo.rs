fn main() {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();
    let process = sys.processes().values().next().unwrap();
    println!("{:?}", process.disk_usage());
    // println!("{:?}", process.user_id()); // sysinfo uses user_id() -> Option<&Uid>
}
