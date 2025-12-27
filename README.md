# oq - Object Query

A jq-like tool for querying and transforming JSON, YAML, TOML, and TOON data.

## Features

- **Auto-detection**: Automatically detects input format (JSON, YAML, TOML, TOON)
- **jq syntax**: Uses familiar jq filter expressions
- **Format conversion**: Convert between any supported formats
- **Streaming**: Reads from files or stdin

## Installation

```bash
cargo install oq
```

## Usage

```bash
# Query JSON (like jq)
echo '{"name": "Ada"}' | oq '.name'
# Output: "Ada"

# Query YAML (auto-detected)
cat config.yaml | oq '.database.host'

# Query TOML
oq '.package.name' Cargo.toml
# Output: "oq"

# Convert formats
cat data.yaml | oq -o json     # YAML -> JSON
cat data.json | oq -o yaml     # JSON -> YAML
cat data.json | oq -o toml     # JSON -> TOML

# Query and convert
oq '.users[]' data.yaml -o json

# Raw string output (no quotes)
oq -r '.name' data.json
```

## Options

```
Usage: oq [OPTIONS] [FILTER] [FILE]...

Arguments:
  [FILTER]   jq filter expression (default: ".")
  [FILE]...  Input files (reads from stdin if not specified)

Options:
  -i, --input <FORMAT>   Input format (auto, json, yaml, toml, toon)
  -o, --output <FORMAT>  Output format (json, yaml, toml, toon)
  -r, --raw              Output raw strings without quotes
  -c, --compact          Compact output (no pretty-printing)
  -s, --slurp            Read entire input as single array
  -n, --null-input       Don't read input, use null
      --color <WHEN>     Colorize output (auto, always, never)
  -h, --help             Print help
  -V, --version          Print version
```

## Supported Formats

| Format | Extensions | Description |
|--------|------------|-------------|
| JSON   | `.json`    | JavaScript Object Notation |
| YAML   | `.yaml`, `.yml` | YAML Ain't Markup Language |
| TOML   | `.toml`    | Tom's Obvious Minimal Language |
| TOON   | `.toon`    | Text Object-Oriented Notation |

## Examples

### Querying Cargo.toml

```bash
# Get package name
oq '.package.name' Cargo.toml

# Get all dependencies
oq '.dependencies | keys' Cargo.toml

# Get dependency versions
oq '.dependencies | to_entries | .[] | "\(.key): \(.value)"' Cargo.toml -r
```

### Converting Formats

```bash
# YAML config to JSON
oq '.' config.yaml -o json > config.json

# JSON API response to YAML
curl -s https://api.example.com/data | oq -o yaml

# Extract and convert
oq '.items' data.json -o yaml
```

### Filter Expressions

oq uses [jaq](https://github.com/01mf02/jaq) for jq-compatible filtering:

```bash
# Identity (output as-is)
oq '.'

# Select field
oq '.name'

# Array index
oq '.[0]'

# Iterate array
oq '.[]'

# Filter
oq '.[] | select(.active == true)'

# Map
oq '.users | map(.name)'

# Combine
oq '.items[] | {name, price}'
```

## Library Usage

```rust
use oq::{parse_auto, compile_filter, run_filter, encode_to_format, Format};

// Parse any format
let value = parse_auto(r#"{"name": "Ada"}"#)?;

// Query with jq expression
let filter = compile_filter(".name")?;
let results = run_filter(&filter, value)?;

// Convert to another format
let yaml = encode_to_format(&results[0], Format::Yaml)?;
```

## License

MIT OR Apache-2.0
