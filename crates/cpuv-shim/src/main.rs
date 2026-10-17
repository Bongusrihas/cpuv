fn main() {
    match cpuv_shim::run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("cpuv-shim: {error:#}");
            std::process::exit(1);
        }
    }
}
