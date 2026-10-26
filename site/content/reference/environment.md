# Environment variables reference

You can configure global behavior for `astli` using environment variables. Command-line flags always take precedence over corresponding environment variables.

| Environment Variable | Equivalent CLI Flag | Default | Description |
| :--- | :--- | :--- | :--- |
| `ASTLI_JOBS` | `--jobs <N>` | `0` (all CPU cores) | Number of parallel worker threads to allocate for parsing, formatting, and analysis. |
| `ASTLI_DIAGNOSTICS` | `--diag <FORMAT>` | `pretty` | Output style for compiler diagnostics and lint reports. Supported values: `pretty` (rich terminal formatting with source context), `short` (`file:line:col: severity[code]: message`), `github` (GitHub Actions workflow commands, which annotate the pull request), `json` (one JSON object per line, and nothing else on stderr), or `gitlab` (a GitLab Code Quality report, and nothing else on stderr). |
| `NO_COLOR` | `--color never` | unset | When set to anything but an empty string, nothing is coloured, even on a terminal. |
| `CLICOLOR_FORCE` | `--color always` | unset | When set to anything but an empty string, output is coloured even when piped. `NO_COLOR` wins over it. |
| `ASTLI_CACHE_DIR` | *(none)* | Platform cache directory | Filesystem path where `astli` downloads, caches, and checks out external Git dependencies. Defaults to `$XDG_CACHE_HOME/astli` (Linux), `~/Library/Caches/astli` (macOS), or `%LOCALAPPDATA%\astli` (Windows). |

!!! tip "Environment variable expansion in filelists"
    `astli` automatically expands environment variables referenced inside EDA filelists (`.f`), supporting both `$VAR` and `${VAR}` syntax. If a referenced environment variable is not set, `astli` raises an error identifying the missing variable.
