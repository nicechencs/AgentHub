fn main() {
    if let Err(error) = agenthub_gui_lib::go_route_bind_probe::main_entry() {
        eprintln!("bind-to-Go probe failed: {error}");
        std::process::exit(1);
    }
}
