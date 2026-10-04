fn main() {
    if let Err(error) = agenthub_gui_lib::plugin_write_probe::main_entry() {
        eprintln!("desktop plugin write probe failed: {error}");
        std::process::exit(1);
    }
}
