fn main() {
    if let Err(error) = agenthub_gui_lib::route_runtime_product_handoff_probe::main_entry() {
        eprintln!("route runtime Product handoff probe failed: {error}");
        std::process::exit(1);
    }
}
