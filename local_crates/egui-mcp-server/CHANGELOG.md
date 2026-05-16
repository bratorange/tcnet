# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.5](https://github.com/dijdzv/egui-mcp/compare/0.0.4...0.0.5) - 2026-01-28

### Refactored

- Split main.rs into modular tools directory ([#33](https://github.com/dijdzv/egui-mcp/pull/33))



## [0.0.4](https://github.com/dijdzv/egui-mcp/compare/0.0.3...0.0.4) - 2026-01-27

### Added

- Add EGUI_MCP_APP_NAME environment variable support ([#18](https://github.com/dijdzv/egui-mcp/pull/18))
- Add CLI with --help, --version, and guide command ([#22](https://github.com/dijdzv/egui-mcp/pull/22))

### Fixed

- Correct repository URL and require subcommand ([#23](https://github.com/dijdzv/egui-mcp/pull/23))

### Performance

- Implement connection reuse for IPC client ([#28](https://github.com/dijdzv/egui-mcp/pull/28))

### Refactored

- Migrate atspi from async-std to tokio ([#24](https://github.com/dijdzv/egui-mcp/pull/24))
- Extract modules from main.rs ([#25](https://github.com/dijdzv/egui-mcp/pull/25))
- Consolidate magic numbers into constants module ([#30](https://github.com/dijdzv/egui-mcp/pull/30))

### Testing

- Expand test coverage with contract-based tests ([#26](https://github.com/dijdzv/egui-mcp/pull/26))



## [0.0.3](https://github.com/dijdzv/egui-mcp/compare/egui-mcp-server-v0.0.2...egui-mcp-server-v0.0.3) - 2026-01-26

### Fixed

- add repository field to all crates for cargo-binstall ([#13](https://github.com/dijdzv/egui-mcp/pull/13))

## [0.0.2](https://github.com/dijdzv/egui-mcp/compare/egui-mcp-server-v0.0.1...egui-mcp-server-v0.0.2) - 2026-01-25

### Added

- implement Phase 6-8 (50+ MCP tools) ([#9](https://github.com/dijdzv/egui-mcp/pull/9))

## [0.0.1](https://github.com/dijdzv/egui-mcp/compare/egui-mcp-server-v0.0.0...egui-mcp-server-v0.0.1) - 2026-01-23

### Other

- add release binary workflow for cargo-binstall support ([#1](https://github.com/dijdzv/egui-mcp/pull/1))
- Phase 5: Implement IPC-based input tools
- Phase 4: Implement click_element and set_text via AT-SPI
- Phase 3: AT-SPI primary architecture and README
- Phase 2: Read Operations and WSLg support
- Phase 1: Implement IPC communication and UI tree extraction
- Initial commit: egui-mcp Phase 0 complete
