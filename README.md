# kbnotes

`kbnotes` is a powerful and flexible knowledge base and note-taking application designed for developers, researchers, and anyone who needs to organize and manage their notes efficiently. It supports features like fuzzy searching, tagging, backups, and more, all accessible through a command-line interface (CLI).

## Features

- **Fuzzy Search**: Quickly find notes by title or content using fuzzy matching.
- **Tagging**: Organize notes with tags for easy categorization and filtering.
- **Backup and Restore**: Create full backups of your notes in a ZIP archive and restore them when needed.
- **CLI Interface**: Manage your notes directly from the command line.
- **Customizable Configuration**: Configure directories, backup settings, and more.
- **Auto-Save and Auto-Backup**: Automatically save and back up your notes to prevent data loss.
- **File Watching**: Automatically detect changes to notes on disk and update the in-memory cache.
- **Editor Integration**: Edit notes using your preferred text editor.

## Installation

1. Ensure you have [Rust](https://www.rust-lang.org/) installed on your system.
2. Clone the repository:
   ```sh
   git clone https://github.com/AllStackDev1/kbnotes.git
   cd kbnotes
   ```
3. Build and install the application:
   ```sh
   cargo build --release
   cargo install --path .
   ```