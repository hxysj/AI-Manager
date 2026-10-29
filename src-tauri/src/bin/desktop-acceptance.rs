fn main() {
    if let Err(error) = monkey_thief_lib::run_desktop_acceptance() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
