use jsondiff::{compare_paths, text_report, MAX_FILES, MIN_FILES};
use std::path::PathBuf;

#[cfg(feature = "gui")]
mod gui;

const USAGE: &str = "Uso: json-diff [--cli] [--all] [-s|--selector <QUERY>] <a.json> <b.json> [... hasta 10]\n\
  Sin argumentos abre la ventana y permite elegir archivos.\n\
  --cli                   imprime el reporte en la terminal en lugar de abrir la ventana\n\
  --all                   (con --cli) incluye también los campos sin cambios\n\
  -s, --selector <QUERY>  filtra la comparación a la subestructura indicada (ej. $.data, $.items[*])";

fn main() {
    let mut cli = false;
    let mut all = false;
    let mut selector: Option<String> = None;
    let mut paths: Vec<PathBuf> = Vec::new();

    let mut args = std::env::args().skip(1).peekable();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--cli" => cli = true,
            "--all" => all = true,
            "-s" | "--selector" => {
                if let Some(val) = args.next() {
                    selector = Some(val);
                } else {
                    eprintln!("Falta el valor para --selector\n{USAGE}");
                    std::process::exit(2);
                }
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            _ => paths.push(PathBuf::from(a)),
        }
    }

    if paths.len() > MAX_FILES {
        eprintln!("Máximo {MAX_FILES} archivos.\n{USAGE}");
        std::process::exit(2);
    }

    if cli || !cfg!(feature = "gui") {
        if paths.len() < MIN_FILES {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
        match compare_paths(&paths) {
            Ok(mut c) => {
                if let Some(s) = selector {
                    if let Err(e) = c.set_selector(&s) {
                        eprintln!("Error en selector: {e}");
                        std::process::exit(2);
                    }
                }
                print!("{}", text_report(&c, !all));
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    #[cfg(feature = "gui")]
    {
        gui::run(paths);
    }
}
