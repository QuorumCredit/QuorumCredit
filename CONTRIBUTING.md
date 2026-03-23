# Contributing to QuorumCredit

Thank you for your interest in contributing to QuorumCredit! We welcome contributions from developers, researchers, and DeFi enthusiasts.

To ensure a smooth collaboration process, please follow these guidelines.

## 🌿 Branch Naming Convention

When creating a new branch, please use one of the following prefixes followed by the issue number or a short description:

| Prefix | Purpose | Example |
|---|---|---|
| `feat/` | New features | `feat/163-add-contributing-guide` |
| `fix/` | Bug fixes | `fix/issue-55-auth-error` |
| `docs/` | Documentation changes | `docs/update-readme-yield` |
| `refactor/` | Code refactoring | `refactor/optimize-vouch-loop` |
| `test/` | Adding/updating tests | `test/add-slash-coverage` |

## 📝 Commit Messages

We follow the [Conventional Commits](https://www.conventionalcommits.org/) specification for our commit messages:

`type: description`

Common types include:
- `feat`: A new feature
- `fix`: A bug fix
- `docs`: Documentation only changes
- `style`: Changes that do not affect the meaning of the code (white-space, formatting, etc.)
- `refactor`: A code change that neither fixes a bug nor adds a feature
- `test`: Adding missing tests or correcting existing tests

**Example:** `feat: add user authentication to request_loan`

## 🚀 Pull Request Process

1. **Fork** the repository and create your branch from `main`.
2. **Code**: Implement your changes.
3. **Test**: Ensure all tests pass locally (see [Testing](#-testing) below).
4. **Style**: Run formatting tools (see [Style Guide](#-style-guide) below).
5. **PR**: Open a Pull Request against the `main` branch.
    - Provide a clear description of the change.
    - Link any related issues (e.g., `Resolves #163`).

## 🧪 Testing

All contributions must pass existing tests. Before submitting your PR, run the following:

```bash
# Run all Soroban contract tests
cargo test

# Run tests with output for debugging
cargo test -- --nocapture
```

If you are adding a new feature, please include corresponding test cases in `src/lib.rs`.

## 🎨 Style Guide

We follow standard Rust formatting conventions. Please run the following before committing:

```bash
cargo fmt --all
```

---

*Happy Coding! 🚀*
