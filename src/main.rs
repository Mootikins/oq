//! oq - Object Query
//!
//! A jq-like tool for JSON, YAML, TOML, and TOON data.
//! Auto-detects input format, queries with jq expressions, outputs in any format.
//!
//! # Usage
//!
//! ```bash
//! # Query JSON (like jq)
//! echo '{"name": "Ada"}' | oq '.name'
//!
//! # Query YAML (auto-detected)
//! cat config.yaml | oq '.database.host'
//!
//! # Query TOML
//! cat Cargo.toml | oq '.package.name'
//!
//! # Convert between formats
//! cat data.yaml | oq -o json    # YAML -> JSON
//! cat data.json | oq -o toml    # JSON -> TOML
//!
//! # Query and convert
//! cat data.yaml | oq '.items[]' -o json
//! ```

use clap::Parser;
use std::io::{self, Read, Write};
use oq::{
    compile_filter, encode_to_format, parse_input, run_filter, CompiledFilter, Format,
    InputFormat, OqError, OutputFormat,
};

#[derive(Parser, Debug)]
#[command(name = "oq")]
#[command(about = "Object Query - jq for JSON, YAML, TOML, and TOON")]
#[command(version)]
#[command(after_help = "EXAMPLES:
    oq '.name' data.json          Query JSON file
    cat config.yaml | oq '.db'    Query YAML from stdin
    oq '.deps' Cargo.toml         Query TOML file
    oq -o yaml '.users' data.json Convert query result to YAML
    oq '.' data.yaml -o json      Convert YAML to JSON")]
struct Cli {
    /// jq filter expression (default: identity ".")
    #[arg(default_value = ".")]
    filter: String,

    /// Input files (reads from stdin if not specified)
    #[arg(value_name = "FILE")]
    files: Vec<String>,

    /// Input format (default: auto-detect)
    #[arg(short = 'i', long = "input", value_name = "FORMAT")]
    input_format: Option<InputFormat>,

    /// Output format (default: same as input, or json for mixed)
    #[arg(short = 'o', long = "output", value_name = "FORMAT")]
    output_format: Option<OutputFormat>,

    /// Output raw strings without quotes
    #[arg(short = 'r', long)]
    raw: bool,

    /// Compact output (no pretty-printing)
    #[arg(short = 'c', long)]
    compact: bool,

    /// Read entire input as single array (like jq -s)
    #[arg(short = 's', long)]
    slurp: bool,

    /// Don't read any input, use null as input
    #[arg(short = 'n', long)]
    null_input: bool,

    /// Colorize output (auto, always, never)
    #[arg(long, default_value = "auto")]
    color: ColorOption,
}

#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
enum ColorOption {
    #[default]
    Auto,
    Always,
    Never,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("oq: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), OqError> {
    let cli = Cli::parse();

    // Configure color output
    match cli.color {
        ColorOption::Always => yansi::enable(),
        ColorOption::Never => yansi::disable(),
        ColorOption::Auto => {
            if !is_terminal() {
                yansi::disable();
            }
        }
    }

    // Compile the filter
    let filter = compile_filter(&cli.filter)?;

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if cli.null_input {
        // Process with null input
        let results = run_filter(&filter, serde_json::Value::Null)?;
        let out_fmt = cli.output_format.map(Format::from).unwrap_or(Format::Json);
        for value in results {
            output_value(&mut out, &value, out_fmt, cli.raw, cli.compact)?;
        }
    } else if cli.files.is_empty() {
        // Read from stdin
        let input = read_stdin()?;
        process_input(&mut out, &input, &cli, &filter)?;
    } else {
        // Read from files
        for path in &cli.files {
            let input = std::fs::read_to_string(path)?;
            process_input(&mut out, &input, &cli, &filter)?;
        }
    }

    Ok(())
}

fn process_input(
    out: &mut impl Write,
    input: &str,
    cli: &Cli,
    filter: &CompiledFilter,
) -> Result<(), OqError> {
    // Detect input format (auto or explicit)
    let input_fmt = cli
        .input_format
        .unwrap_or(InputFormat::Auto)
        .detect(input);

    // Parse input
    let value = parse_input(input, input_fmt)?;

    // Run the filter
    let results = run_filter(filter, value)?;

    // Determine output format: explicit > input format > json
    let output_fmt = cli
        .output_format
        .map(Format::from)
        .unwrap_or(input_fmt);

    // Output results
    for value in results {
        output_value(out, &value, output_fmt, cli.raw, cli.compact)?;
    }

    Ok(())
}

fn output_value(
    out: &mut impl Write,
    value: &serde_json::Value,
    format: Format,
    raw: bool,
    compact: bool,
) -> Result<(), OqError> {
    // Raw string output (like jq -r)
    if raw {
        if let serde_json::Value::String(s) = value {
            writeln!(out, "{}", s)?;
            return Ok(());
        }
    }

    // Format based on output format
    // TOML can only encode objects, fall back to JSON for primitives
    let effective_format = match format {
        Format::Toml if !value.is_object() => Format::Json,
        other => other,
    };

    let output = match effective_format {
        Format::Json => {
            if compact {
                serde_json::to_string(value)?
            } else {
                serde_json::to_string_pretty(value)?
            }
        }
        _ => encode_to_format(value, effective_format)?,
    };

    writeln!(out, "{}", output)?;
    Ok(())
}

fn read_stdin() -> Result<String, io::Error> {
    let mut input = String::new();
    io::stdin().lock().read_to_string(&mut input)?;
    Ok(input)
}

fn is_terminal() -> bool {
    std::env::var("TERM").is_ok()
}
