# Kakapo

A Windows application for viewing system windows and sending keystrokes to them. Built with [GPUI](https://github.com/zed-industries/zed) framework.

## Features

- **Window List**: View all visible windows on your system
- **Window Selection**: Click any window to select it as the target
- **Keystroke Input**: Type text in the input field to send to the selected window
- **Virtual Keystroke Support**: Send keystrokes that work with virtual machines and remote desktop applications
- **Typing Speed Control**: Configure typing speed in characters per minute (default: 300 CPM, ~60 WPM)
- **Natural Typing Variability**: Enable/disable jitter for more realistic, human-like typing patterns with "spurt" behavior
- **Resume**: Pause automatically when the target window loses focus and resume when focus returns
- **Auto-Pause on Focus Loss**: Typing automatically pauses when the target window loses focus and resumes when focus returns
- **Stop**: Cancel typing at any time with the Stop button

## How It Works

Kakapo uses the Windows `SendInput` API with the `KEYEVENTF_UNICODE` flag to send keystrokes at the lowest level. This approach ensures compatibility with:

- Virtual machines (VMware, VirtualBox, Hyper-V)
- Remote desktop applications (Amazon Workspaces, Azure Virtual Desktop)
- Standard Windows applications

### Typing Variability

The application simulates realistic human typing behavior through:

- **Configurable Speed**: Set your desired typing speed in characters per minute
- **Jitter Pattern**: When enabled, creates natural variations in typing speed that mimic human behavior
- **Spurt Behavior**: Alternates between faster and slower typing periods (5-15 characters each), creating a more natural rhythm
- **Random Variation**: Adds 80-120% variation to each keystroke timing within spurts

## Usage

1. **Launch the application**: Run `cargo run --release` or execute the built binary
2. **Select a target window**: Click on any window in the list to select it
3. **Configure typing speed** (optional): Enter desired characters per minute in the input field
4. **Enable/disable jitter** (optional): Click the "Jitter" button to toggle natural typing variability
5. **Type your text**: Enter the text you want to send in the input field
6. **Send keystrokes**: Click the "Send" button
7. **Resume** (optional): If typing is paused (manually or due to focus loss), click "Resume" to continue
   - Typing automatically pauses if the target window loses focus and resumes when focus returns
8. **Stop** (optional): Click the "Stop" button to cancel typing completely

## Building

### Prerequisites

- Rust toolchain (1.93.0 or compatible)
- Windows operating system

### Build Instructions

```bash
# Clone the repository
git clone https://github.com/dalyIsaac/kakapo.git
cd kakapo

# Build the project
cargo build --release

# Run the application
cargo run --release
```

## Technical Details

### Virtual Keystroke Implementation

The keystroke sending functionality is inspired by [KeePassXC's approach](https://github.com/keepassxreboot/keepassxc) to keystroke injection. Key technical details:

1. **SetForegroundWindow**: Brings the target window to the foreground
2. **SendInput with KEYEVENTF_UNICODE**: Sends each character as a Unicode keystroke
3. **Key Events**: Each character is sent as a key-down followed by key-up event
4. **Timing**: Small delays between characters ensure reliability
5. **Pause Functionality**: Typing can be paused manually or automatically when focus is lost
6. **Stop Functionality**: Typing can be cancelled mid-operation using the Stop button
7. **Focus Monitoring**: Continuously checks if the target window has focus and auto-pauses when lost

### Why KEYEVENTF_UNICODE?

The `KEYEVENTF_UNICODE` flag tells Windows to inject keystrokes at the lowest level, bypassing most keyboard hooks and filters. This makes it work reliably with:

- Applications running in virtual machines
- Remote desktop sessions
- Applications with custom keyboard handling

### Dependencies

- **gpui**: GPU-accelerated UI framework from Zed
- **windows**: Windows API bindings for Rust

## Limitations

- **Windows Only**: This application uses Windows-specific APIs and will not run on other operating systems
- **Foreground Focus**: The target window is brought to the foreground to receive keystrokes
- **No Special Keys**: Currently only supports sending Unicode characters (letters, numbers, symbols) and Enter key

## CI/CD

This project uses GitHub Actions for continuous integration and deployment:

- **Clippy Workflow**: Runs on all pushes and pull requests to the main branch to ensure code quality using Rust's clippy linter
- **Release Workflow**: Automatically builds and creates GitHub releases for all commits to the main branch
  - Builds a release binary for Windows
  - Creates a GitHub release with auto-generated release notes
  - Uploads the compiled `kakapo.exe` as a release asset
  - Tag format: `v{version}-{commit-sha}` (e.g., `v0.1.0-a8883b6`)

## License

See LICENSE file for details.

## Acknowledgments

- Inspired by [KeePassXC](https://github.com/keepassxreboot/keepassxc) for the keystroke injection approach
- Built with [GPUI](https://github.com/zed-industries/zed) framework
