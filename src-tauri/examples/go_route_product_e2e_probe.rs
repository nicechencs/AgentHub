fn main() {
    if let Err(error) = agenthub_gui_lib::go_route_product_probe::main_entry() {
        eprintln!("product Go route probe failed: {error}");
        std::process::exit(1);
    }
}
