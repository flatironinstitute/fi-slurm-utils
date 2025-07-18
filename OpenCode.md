# OpenCode.md

## 1. Build, Lint, and Test Commands

- To build: *Use `cargo build` in order to build in debug mode. Only build after successfully running `cargo check` with no errors, warnings, or hints.*
- To lint: *Use `cargo clippy` for linting.*
- To test: *Use `cargo test` to test all binaries across the project.*
- Run a single test: *Use `cargo test --bin {binary name}` to test a particular binary.*
- Run benchmarks: *If a benchmarking directory is present, run `cargo bench` to benchmark using Criterion.*

## 2. Codebase Code of Conduct

- **Mission** The details of the codebase and the current project are located in the project.md file. Refer to that file for detailed information on the codebase, which the user will reference when giving task instructions.
- **Permissions** All projects will have a .opencode.json configuration file and an OpenCode.md guidelines file at the project root. Under no circumstances should you edit or delete files of directories above the project root. If any instruction would cause you to edit or delete a file or repository above the project root, exit the session with an explanation of the series of events that led to this.
- **Version Control** Use Git for all version control. If there is a remote repository, use `git fetch` and `git pull` before large changes to ensure that the current branch is up to date with its remote version. In the event that edits have been made, but not committed, and the remote branch is ahead of the local branch, stash the current changes with an informative message, pull the remote commits, and then pop the stash, making sure to adapt uncommitted changes to the new branch state.
- **Branching** All projects will have a `main` or `master` branch, as well as feature branches. Feature branches will be created by the project manager. Work directly on these feature branches, except for risky or experimental changes, for which you may create tertiary branches at will. Tertiary branches may merge into their parent feature branch only after a full cycle of formatting and linting has successfully occurred with no errors, warnings, or hints. Do not push changes to feature branches to `main` or `master`. 
- **Commit Messages:** Clearly describe the *why*, not just the *what*.
- **Configuration:** Put configuration in environment variables or config files, not hardcoded.
- **General:** Keep code minimal, idiomatic, and easy to follow. Prefer fixing root causes.

## 3. Code Style Guidelines

- **Formatting:** Use the standard `rustfmt` formatter.
- **Imports:** Place standard library imports first, then third-party packages, then local modules.
- **Types:** Prefer explicit type annotations where it improves clarity. Prefer descriptive enums to boolean configuration variables. When a function would return three or more objects, prefer to return a custom struct with an informative name.
- **Naming:** Use descriptive, consistent names (snake_case for variables/functions, PascalCase for types/structs/enums).
- **Organization:** Place function, struct, and enum declarations before any other objects that include them. The definition of any function, struct, or enum should always be its global declaration within the file.
- **Error Handling:** Always handle errors explicitly using Result or Option as appropriate. 
- **Documentation:** Add concise docstrings/comments for all public interfaces and non-obvious logic. Ensure that any function marked `unsafe` has a `# Safety` header with information about the potentially unsafe behavior ofthe function. Do not put periods at the end of comments.

## 4. Special Notes

This development environment is a local repository linked to a remote repository which feeds into the live application on a target server. Because the local development environment does not have a compiled `libslurm.so` binary, some LSP errors which would show up on the target server do not show up here. This development environment is a staging ground for changes which will be refined and confirmed on the target server.  

As a result, is it *not possible* to check or build the application in the current development environment. Rely instead on formatiing and linting, as well as LSP feedback.
