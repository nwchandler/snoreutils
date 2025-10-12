use std::env;
use std::io;
use std::process;

fn main() {
    let mut args = env::args().skip(1);
    let mut stdout = io::stdout().lock();

    let mut exit_code = 0;
    match args.next() {
        Some(path) => {
            if let Err(e) = cat::file(&path, &mut stdout) {
                eprintln!("cat: {path}: {e}");
                exit_code = 1;
            };
            for path in args {
                if let Err(e) = cat::file(&path, &mut stdout) {
                    eprintln!("cat: {path}: {e}");
                    exit_code = 1;
                };
            }
        }
        None => {
            let stdin = io::stdin().lock();
            if let Err(e) = cat::stream(stdin, stdout) {
                eprintln!("cat: {e}");
                exit_code = 1;
            };
        }
    };

    process::exit(exit_code);
}
