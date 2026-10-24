use std::process::ExitCode;

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

// Never returns `Result`: a `main` that returns `Err` prints its `Debug` to stderr.
fn main() -> ExitCode {
    let command = server::parse(std::env::args_os().skip(1));
    match server::run_cli(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => server::exit_code(&error),
    }
}
