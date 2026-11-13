mod command;

fn main() -> std::process::ExitCode {
    match command::execute(std::env::args_os().skip(1)) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("STE driver: {error}");
            std::process::ExitCode::from(1)
        }
    }
}
