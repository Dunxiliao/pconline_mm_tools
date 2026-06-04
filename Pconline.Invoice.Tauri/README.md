# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)


npm run tauri dev

npm run tauri build

## macOS 安装包（GitHub Actions）

Windows 请继续在本机执行 `.\build-prod.ps1`。macOS 见 [docs/BUILD_MACOS_GITHUB.md](docs/BUILD_MACOS_GITHUB.md)。

## Windows 发版（本机）

以后发新版本时：
改 src-tauri/tauri.conf.json 的 "version"。
（可选）把 package.json 的 "version" 改成同一版本。
再执行 `.\build-prod.ps1`，生成的 MSI 文件名里的版本就会是新版本号。
说明： Cargo.toml 里的 version = "0.1.0" 是 Rust 包版本，不参与安装包命名，可以不随发版改。