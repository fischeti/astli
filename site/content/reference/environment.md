# Environment variables reference

You can configure global behavior for `astli` using environment variables. Command-line flags always take precedence over corresponding environment variables.

| Environment Variable | Equivalent CLI Flag | Default | Description |
| :--- | :--- | :--- | :--- |
| `ASTLI_JOBS` | `--jobs <N>` | `0` (all CPU cores) | Number of parallel worker threads to allocate for parsing, formatting, and analysis. |
| `ASTLI_DIAGNOSTICS` | `--diag <FORMAT>` | `pretty` | Output style for compiler diagnostics and lint reports. Supported values: `pretty` (rich terminal formatting with source context) or `short` (`file:line:col: severity[code]: message`). |
| `ASTLI_CACHE_DIR` | *(none)* | Platform cache directory | Filesystem path where `astli` downloads, caches, and checks out external Git dependencies. Defaults to `$XDG_CACHE_HOME/astli` (Linux), `~/Library/Caches/astli` (macOS), or `%LOCALAPPDATA%\astli` (Windows). |

!!! tip "Environment variable expansion in filelists"
    `astli` automatically expands environment variables referenced inside EDA filelists (`.f`), supporting both `$VAR` and `${VAR}` syntax. If a referenced environment variable is not set, `astli` raises an error identifying the missing variable.
