#!/usr/bin/env bash
#
# Extracts Annex A -- the formal syntax -- from a copy of IEEE 1800-2023 into
# grammar/annex-a.bnf, and the names of its productions into
# grammar/productions.txt.
#
# The BNF is a reading aid: grepping `data_declaration ::=` while writing a
# rule beats paging through a PDF. It is *not* what SyntaxKind is generated
# from, and nothing builds against it -- see docs/plan.md D11 for why. The name
# list is the coverage checklist, and is the only one of the two that is
# committed: a list of names is fact, a transcription of the annex is the
# standard's text.
#
# The PDF is an argument rather than a path in here, because the copy this was
# written against is gitignored and a committed script may not point at it.
#
# Usage: scripts/extract-grammar.sh path/to/1800-2023.pdf

set -euo pipefail

if [ $# -ne 1 ]; then
    echo "usage: $0 path/to/1800-2023.pdf" >&2
    exit 2
fi

pdf="$1"
command -v pdftotext >/dev/null || { echo "need pdftotext (poppler)" >&2; exit 1; }
[ -f "$pdf" ] || { echo "no such file: $pdf" >&2; exit 1; }

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$root/grammar"
mkdir -p "$out"

# The 56 footnotes are kept, as prose at the end of the file, but their markers
# are not: a marker is indistinguishable from part of the name it sits on once
# the PDF's superscripts are flattened. So a production no longer points at its
# footnote, and the constraints they carry have to be found by reading them.
#
# Footnote superscripts land glued to the name they mark -- `expression33`,
# `timeunits_declaration3` -- and stripping every trailing digit would eat the
# names that really end in one. Annex A's are these 31, and they are closed:
# 27 are reserved words, and the rest are `delay2`/`delay3`/`strength0`/
# `strength1` plus the `1'b0` literal forms. Footnotes are numbered from 1, so
# nothing suffixed `0` is ever one of them.
keep='b0|b1|B0|B1|Z0|bufif0|bufif1|delay2|delay3|highz0|highz1|notif0|notif1'
keep="$keep"'|pull0|pull1|rtranif0|rtranif1|strength0|strength1|strong0|strong1'
keep="$keep"'|supply0|supply1|tranif0|tranif1|tri0|tri1|unique0|weak0|weak1'

pdftotext -layout "$pdf" - | awk -v keep="$keep" '
    # Annex A runs from its first section heading to the next annex. Both
    # anchors are whole lines; the table of contents spells them with trailing
    # dots and a page number, so it does not match.
    /^[[:space:]]*A\.1 Source text[[:space:]]*$/ { inside = 1 }
    /^[[:space:]]*Annex B[[:space:]]*$/          { inside = 0 }
    !inside { next }

    # The running header and the footer, on all 47 pages. The footer wraps
    # into three ragged fragments rather than one line, which is why this is a
    # list of scraps and not a tidy pattern.
    /Copyright|IEEE Std|IEEE Standard for SystemVerilog|Authorized licensed/ { next }
    /All rights|rights reserved|reserved\.|Downloaded on|IEEE Xplore|Restrictions apply/ { next }
    /^[[:space:]]*[0-9]+[[:space:]]*$/ { next }

    {
        # Strip footnote markers. A marker is a digit run ending a *whole*
        # name, so the identifier is taken first and its tail examined after:
        # matching the digits directly would find `t01` inside
        # `t01_path_delay_expression` and leave `t_path_delay_expression`.
        line = $0
        out = ""
        while (match(line, /[A-Za-z_$][A-Za-z0-9_$]*/)) {
            name = substr(line, RSTART, RLENGTH)
            out = out substr(line, 1, RSTART - 1)
            line = substr(line, RSTART + RLENGTH)
            bare = name
            sub(/[0-9]+$/, "", bare)
            if (bare != "" && bare != name && name !~ ("^(" keep ")$")) name = bare
            out = out name
        }
        $0 = out line
        # A marker on a closing brace rather than on a name.
        gsub(/\}[0-9]+/, "}")

        # Collapse the blank runs left by page breaks.
        if ($0 ~ /^[[:space:]]*$/) { blank = 1; next }
        if (blank && seen) print ""
        blank = 0; seen = 1
        sub(/^[[:space:]]{5}/, "")
        print
    }
' > "$out/annex-a.bnf"

grep -oE '^[a-zA-Z_$][a-zA-Z0-9_$]*[[:space:]]*::=' "$out/annex-a.bnf" |
    sed 's/[[:space:]]*::=//' | sort -u > "$out/productions.txt"

printf '%s: %s productions, %s lines\n' \
    "$(basename "$pdf")" \
    "$(wc -l < "$out/productions.txt" | tr -d ' ')" \
    "$(wc -l < "$out/annex-a.bnf" | tr -d ' ')"
