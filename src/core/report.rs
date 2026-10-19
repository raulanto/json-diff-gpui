//! Generación de reportes de diferencias en formatos texto plano (CLI), Markdown y CSV.

use crate::core::comparison::Comparison;
use crate::core::types::DiffMode;

/// Reporte en texto plano (modo `--cli`).
pub fn text_report(c: &Comparison, only_changed: bool) -> String {
    let mut out = String::new();
    for (i, f) in c.files.iter().enumerate() {
        out.push_str(&format!("[{}] {}", i + 1, f.path.display()));
        match c.diff_mode {
            DiffMode::FixedBase => {
                if i == c.base_index {
                    out.push_str("  (base)");
                }
            }
            DiffMode::Sequential => {
                if i == 0 {
                    out.push_str("  (origen)");
                } else {
                    out.push_str(&format!("  (vs [{}])", i));
                }
            }
        }
        if !f.lines_exact {
            out.push_str("  (líneas según formato normalizado)");
        }
        out.push('\n');
    }
    if c.selector != "$" {
        out.push_str(&format!("Selector: {}\n", c.selector));
    }
    out.push_str(&format!(
        "\n{} de {} campos con diferencias ({})\n\n",
        c.changed_count(),
        c.rows.len(),
        match c.diff_mode {
            DiffMode::FixedBase => "Modo: Base fija",
            DiffMode::Sequential => "Modo: En cadena secuencial",
        }
    ));
    for r in c.rows.iter().filter(|r| !only_changed || r.changed) {
        out.push_str(&format!(
            "{} {}\n",
            if r.changed { "≠" } else { "=" },
            r.path
        ));
        for (i, cell) in r.cells.iter().enumerate() {
            match cell {
                Some(cell) => {
                    out.push_str(&format!("    [{}] L{}: {}\n", i + 1, cell.line, cell.value))
                }
                None => out.push_str(&format!("    [{}] (no existe)\n", i + 1)),
            }
        }
    }
    out
}

/// Exporta el reporte en formato Markdown tabular enriquecido.
pub fn export_markdown(c: &Comparison, only_changed: bool) -> String {
    let mut out = String::new();
    out.push_str("# Reporte de Diferencias JSON\n\n");
    if c.selector != "$" {
        out.push_str(&format!("**Selector de estructura:** `{}`\n\n", c.selector));
    }
    match c.diff_mode {
        DiffMode::FixedBase => {
            out.push_str(&format!(
                "**{} de {} campos con diferencias** (Base: [{}] `{}`)\n\n",
                c.changed_count(),
                c.rows.len(),
                c.base_index + 1,
                c.files[c.base_index].name
            ));
        }
        DiffMode::Sequential => {
            out.push_str(&format!(
                "**{} de {} campos con diferencias** (Modo: En cadena secuencial [1] -> ... -> [{}])\n\n",
                c.changed_count(),
                c.rows.len(),
                c.files.len()
            ));
        }
    }

    out.push_str("| Estado | Campo |");
    for (i, f) in c.files.iter().enumerate() {
        let tag = match c.diff_mode {
            DiffMode::FixedBase => {
                if i == c.base_index {
                    " *(base)*".to_string()
                } else {
                    String::new()
                }
            }
            DiffMode::Sequential => {
                if i == 0 {
                    " *(origen)*".to_string()
                } else {
                    format!(" *(vs [{}])*", i)
                }
            }
        };
        out.push_str(&format!(" [{}] {}{} |", i + 1, f.name, tag));
    }
    out.push('\n');

    out.push_str("|:---:|:---|");
    for _ in &c.files {
        out.push_str(":---|");
    }
    out.push('\n');

    for r in c.rows.iter().filter(|r| !only_changed || r.changed) {
        out.push_str(&format!(
            "| {} | `{}` |",
            if r.changed { "≠" } else { "=" },
            r.path
        ));
        for cell in &r.cells {
            match cell {
                Some(c) => {
                    out.push_str(&format!(" L{} `{}` |", c.line, c.value.replace('|', "\\|")))
                }
                None => out.push_str(" *(no existe)* |"),
            }
        }
        out.push('\n');
    }
    out
}

/// Exporta el reporte en formato CSV compatible con hojas de cálculo.
pub fn export_csv(c: &Comparison, only_changed: bool) -> String {
    let mut out = String::new();
    out.push_str("Diferencia,Campo");
    for (i, f) in c.files.iter().enumerate() {
        let is_base = if i == c.base_index { " (Base)" } else { "" };
        out.push_str(&format!(
            ",\"[{}] {}{}\"",
            i + 1,
            f.name.replace('"', "\"\""),
            is_base
        ));
    }
    out.push('\n');

    for r in c.rows.iter().filter(|r| !only_changed || r.changed) {
        out.push_str(&format!(
            "{},\"{}\"",
            if r.changed { "SI" } else { "NO" },
            r.path.replace('"', "\"\"")
        ));
        for cell in &r.cells {
            match cell {
                Some(c) => {
                    let text = format!("L{}: {}", c.line, c.value);
                    out.push_str(&format!(",\"{}\"", text.replace('"', "\"\"")));
                }
                None => out.push_str(",\"(no existe)\""),
            }
        }
        out.push('\n');
    }
    out
}
