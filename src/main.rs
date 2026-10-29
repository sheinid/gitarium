use clap::Parser;
use gitarium::cli::{self, Cli, Format};
fn main() {
    let args = match Cli::try_parse() {
        Ok(args) => args,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            let _ = error.print();
            return;
        }
        Err(error) => {
            let args: Vec<String> = std::env::args().collect();
            let human = args
                .windows(2)
                .any(|a| a[0] == "--format" && a[1] == "human")
                || args.iter().any(|a| a == "--format=human");
            let json = args
                .windows(2)
                .any(|a| a[0] == "--format" && a[1] == "json")
                || args.iter().any(|a| a == "--format=json");
            if !human && (json || cli::json_output(Format::Auto)) {
                println!(
                    "{}",
                    serde_json::json!({"ok":false,"error":{"code":"validation_error","message":error.to_string(),"retryable":false},"meta":{"schema_version":"1.0.0"}})
                );
            } else {
                let _ = error.print();
            }
            std::process::exit(2);
        }
    };
    std::process::exit(cli::emit(cli::execute(&args), args.format));
}
