# Flint

> This contains only implementation details and user/packager instructions, for a more indepth view, look at `ARCHITECTURE.md`

Flint is the universal package manager.

## Installation

### Via flint

Replace `REPO_NAME` and `REPO_URL` with your desired initial repository.
The initial repository MUST contain flint.
```bash
curl https://raw.githubusercontent.com/Taxrosdev/Flint/refs/heads/main/install_standalone.sh -o /tmp/flint_script.sh
chmod 700 /tmp/flint_script.sh
/tmp/flint_script.sh REPO_NAME REPO_URL
rm /tmp/flint_script.sh
```

### Cargo

```bash
cargo install flintpkg --locked
```
