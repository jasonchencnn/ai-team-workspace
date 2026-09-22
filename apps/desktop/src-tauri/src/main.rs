#![forbid(unsafe_code)]

fn main() {
    if let Err(error) = desktop::run() {
        eprintln!("desktop host stopped unexpectedly: {error}");
        std::process::exit(1);
    }
}
