# MLS GigaChad - AI Programming & Planning Agent

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

MLS GigaChad is an AI-powered programming assistant that helps with technical planning, research, and code generation. It combines web search capabilities with structured reasoning to create detailed technical plans and implementation strategies.

![Web Interface Screenshot](static/screenshot.png) *(Example screenshot placeholder)*

## Features

- **Multi-phase Planning**:
  - Foundational research
  - Component analysis
  - Synthesis of findings
- **Web Research**: Integrated DuckDuckGo search for technical information
- **Dual Interface**:
  - Web server with modern UI (port 8000)
  - Interactive CLI mode
- **AI Integration**: DeepSeek API for advanced reasoning
- **Markdown Support**: Rich formatting in responses

## Installation

### Prerequisites
- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- DeepSeek API key

### Steps
```bash
git clone https://github.com/yourusername/mls-gigachad.git
cd mls-gigachad

# Set up environment
echo "DEEPSEEK_API_KEY=your_key_here" > .env

# Build and run
cargo build --release
cargo run -- server  # Web mode
# or
cargo run -- cli     # Interactive CLI
```

## Usage

### Web Interface
Access at `http://localhost:8000` after starting in server mode.

### API Endpoints
**Create Technical Plan** (`POST /planner/create_plan`):
```json
{
  "messages": [
    {
      "role": "user",
      "content": "Build a Rust web scraper for product data"
    }
  ]
}
```

**Chat Interface** (`POST /planner/chat`):
```json
{
  "messages": [
    {
      "role": "user",
      "content": "How do I parse HTML in Rust?"
    }
  ]
}
```

### CLI Mode
Interactive session example:
```
> I need to build a REST API in Rust
Choose mode: (c)hat, (p)lan, (b)ack, (r)eset
[p]> p

Creating plan... (this may take a moment)
Press Ctrl+C to cancel

Assistant: Here's the technical plan for your Rust REST API:
1. Framework selection (Actix-web vs Rocket)
2. Database integration
3. Error handling strategy
...
```

## Project Structure

```
├── src/
│   ├── main.rs          # Core application logic
├── static/
│   ├── index.html       # Web interface
├── .gitignore
├── Cargo.toml           # Rust dependencies
├── Cargo.lock
├── README.md
```

## Configuration

Environment variables (`.env` file):
```env
DEEPSEEK_API_KEY=your_api_key
PORT=8000               # Optional
MAX_SEARCHES=50         # Max web searches per plan
```

## Development

```bash
# Run tests
cargo test

# Format code
cargo fmt

# Check for warnings
cargo clippy
```

## Roadmap

- [ ] Add persistent conversation history
- [ ] Support for multiple AI providers
- [ ] Enhanced web interface with conversation threading
- [ ] Plugin system for additional research tools

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/foo`)
3. Commit changes (`git commit -am 'Add foo'`)
4. Push branch (`git push origin feature/foo`)
5. Open a Pull Request

## License

MIT - See [LICENSE](LICENSE) for details.
