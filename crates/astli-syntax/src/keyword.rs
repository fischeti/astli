//! Reserved keyword definitions and lookup tables for SystemVerilog.

use std::sync::LazyLock;

use rustc_hash::FxHashMap;

use crate::{SyntaxKind, SyntaxKind::*};

/// SystemVerilog language standard version for keyword resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeywordVersion {
    /// IEEE 1800-2023 reserved keywords (Annex B).
    #[default]
    V1800_2023,
}

/// Lookup table for IEEE 1800-2023 keywords.
static INDEX_1800_2023: LazyLock<FxHashMap<&'static str, SyntaxKind>> =
    LazyLock::new(|| KEYWORDS_1800_2023.iter().copied().collect());

/// Resolves an identifier against the keyword table for the given language version.
///
/// Returns `Some(SyntaxKind)` if `ident` matches a reserved keyword, or `None` if
/// it is a regular identifier.
pub(crate) fn lookup(ident: &str, version: KeywordVersion) -> Option<SyntaxKind> {
    let index = match version {
        KeywordVersion::V1800_2023 => &*INDEX_1800_2023,
    };
    index.get(ident).copied()
}

impl SyntaxKind {
    /// Returns the spelling of a keyword kind, or `None` if it is not a keyword.
    pub fn keyword_text(self) -> Option<&'static str> {
        if self == ONE_STEP_KW {
            return Some("1step");
        }
        KEYWORDS_1800_2023
            .iter()
            .find(|&&(_, k)| k == self)
            .map(|&(text, _)| text)
    }
}

/// IEEE 1800-2023 reserved keywords (Annex B) and their associated syntax kinds.
///
/// Note: `1step` is omitted because it begins with a digit and is lexed
/// directly as [`SyntaxKind::ONE_STEP_KW`].
const KEYWORDS_1800_2023: &[(&str, SyntaxKind)] = &[
    ("accept_on", ACCEPT_ON_KW),
    ("alias", ALIAS_KW),
    ("always", ALWAYS_KW),
    ("always_comb", ALWAYS_COMB_KW),
    ("always_ff", ALWAYS_FF_KW),
    ("always_latch", ALWAYS_LATCH_KW),
    ("and", AND_KW),
    ("assert", ASSERT_KW),
    ("assign", ASSIGN_KW),
    ("assume", ASSUME_KW),
    ("automatic", AUTOMATIC_KW),
    ("before", BEFORE_KW),
    ("begin", BEGIN_KW),
    ("bind", BIND_KW),
    ("bins", BINS_KW),
    ("binsof", BINSOF_KW),
    ("bit", BIT_KW),
    ("break", BREAK_KW),
    ("buf", BUF_KW),
    ("bufif0", BUFIF0_KW),
    ("bufif1", BUFIF1_KW),
    ("byte", BYTE_KW),
    ("case", CASE_KW),
    ("casex", CASEX_KW),
    ("casez", CASEZ_KW),
    ("cell", CELL_KW),
    ("chandle", CHANDLE_KW),
    ("checker", CHECKER_KW),
    ("class", CLASS_KW),
    ("clocking", CLOCKING_KW),
    ("cmos", CMOS_KW),
    ("config", CONFIG_KW),
    ("const", CONST_KW),
    ("constraint", CONSTRAINT_KW),
    ("context", CONTEXT_KW),
    ("continue", CONTINUE_KW),
    ("cover", COVER_KW),
    ("covergroup", COVERGROUP_KW),
    ("coverpoint", COVERPOINT_KW),
    ("cross", CROSS_KW),
    ("deassign", DEASSIGN_KW),
    ("default", DEFAULT_KW),
    ("defparam", DEFPARAM_KW),
    ("design", DESIGN_KW),
    ("disable", DISABLE_KW),
    ("dist", DIST_KW),
    ("do", DO_KW),
    ("edge", EDGE_KW),
    ("else", ELSE_KW),
    ("end", END_KW),
    ("endcase", ENDCASE_KW),
    ("endchecker", ENDCHECKER_KW),
    ("endclass", ENDCLASS_KW),
    ("endclocking", ENDCLOCKING_KW),
    ("endconfig", ENDCONFIG_KW),
    ("endfunction", ENDFUNCTION_KW),
    ("endgenerate", ENDGENERATE_KW),
    ("endgroup", ENDGROUP_KW),
    ("endinterface", ENDINTERFACE_KW),
    ("endmodule", ENDMODULE_KW),
    ("endpackage", ENDPACKAGE_KW),
    ("endprimitive", ENDPRIMITIVE_KW),
    ("endprogram", ENDPROGRAM_KW),
    ("endproperty", ENDPROPERTY_KW),
    ("endsequence", ENDSEQUENCE_KW),
    ("endspecify", ENDSPECIFY_KW),
    ("endtable", ENDTABLE_KW),
    ("endtask", ENDTASK_KW),
    ("enum", ENUM_KW),
    ("event", EVENT_KW),
    ("eventually", EVENTUALLY_KW),
    ("expect", EXPECT_KW),
    ("export", EXPORT_KW),
    ("extends", EXTENDS_KW),
    ("extern", EXTERN_KW),
    ("final", FINAL_KW),
    ("first_match", FIRST_MATCH_KW),
    ("for", FOR_KW),
    ("force", FORCE_KW),
    ("foreach", FOREACH_KW),
    ("forever", FOREVER_KW),
    ("fork", FORK_KW),
    ("forkjoin", FORKJOIN_KW),
    ("function", FUNCTION_KW),
    ("generate", GENERATE_KW),
    ("genvar", GENVAR_KW),
    ("global", GLOBAL_KW),
    ("highz0", HIGHZ0_KW),
    ("highz1", HIGHZ1_KW),
    ("if", IF_KW),
    ("iff", IFF_KW),
    ("ifnone", IFNONE_KW),
    ("ignore_bins", IGNORE_BINS_KW),
    ("illegal_bins", ILLEGAL_BINS_KW),
    ("implements", IMPLEMENTS_KW),
    ("implies", IMPLIES_KW),
    ("import", IMPORT_KW),
    ("incdir", INCDIR_KW),
    ("include", INCLUDE_KW),
    ("initial", INITIAL_KW),
    ("inout", INOUT_KW),
    ("input", INPUT_KW),
    ("inside", INSIDE_KW),
    ("instance", INSTANCE_KW),
    ("int", INT_KW),
    ("integer", INTEGER_KW),
    ("interconnect", INTERCONNECT_KW),
    ("interface", INTERFACE_KW),
    ("intersect", INTERSECT_KW),
    ("join", JOIN_KW),
    ("join_any", JOIN_ANY_KW),
    ("join_none", JOIN_NONE_KW),
    ("large", LARGE_KW),
    ("let", LET_KW),
    ("liblist", LIBLIST_KW),
    ("library", LIBRARY_KW),
    ("local", LOCAL_KW),
    ("localparam", LOCALPARAM_KW),
    ("logic", LOGIC_KW),
    ("longint", LONGINT_KW),
    ("macromodule", MACROMODULE_KW),
    ("matches", MATCHES_KW),
    ("medium", MEDIUM_KW),
    ("modport", MODPORT_KW),
    ("module", MODULE_KW),
    ("nand", NAND_KW),
    ("negedge", NEGEDGE_KW),
    ("nettype", NETTYPE_KW),
    ("new", NEW_KW),
    ("nexttime", NEXTTIME_KW),
    ("nmos", NMOS_KW),
    ("nor", NOR_KW),
    ("noshowcancelled", NOSHOWCANCELLED_KW),
    ("not", NOT_KW),
    ("notif0", NOTIF0_KW),
    ("notif1", NOTIF1_KW),
    ("null", NULL_KW),
    ("or", OR_KW),
    ("output", OUTPUT_KW),
    ("package", PACKAGE_KW),
    ("packed", PACKED_KW),
    ("parameter", PARAMETER_KW),
    ("pmos", PMOS_KW),
    ("posedge", POSEDGE_KW),
    ("primitive", PRIMITIVE_KW),
    ("priority", PRIORITY_KW),
    ("program", PROGRAM_KW),
    ("property", PROPERTY_KW),
    ("protected", PROTECTED_KW),
    ("pull0", PULL0_KW),
    ("pull1", PULL1_KW),
    ("pulldown", PULLDOWN_KW),
    ("pullup", PULLUP_KW),
    ("pulsestyle_ondetect", PULSESTYLE_ONDETECT_KW),
    ("pulsestyle_onevent", PULSESTYLE_ONEVENT_KW),
    ("pure", PURE_KW),
    ("rand", RAND_KW),
    ("randc", RANDC_KW),
    ("randcase", RANDCASE_KW),
    ("randsequence", RANDSEQUENCE_KW),
    ("rcmos", RCMOS_KW),
    ("real", REAL_KW),
    ("realtime", REALTIME_KW),
    ("ref", REF_KW),
    ("reg", REG_KW),
    ("reject_on", REJECT_ON_KW),
    ("release", RELEASE_KW),
    ("repeat", REPEAT_KW),
    ("restrict", RESTRICT_KW),
    ("return", RETURN_KW),
    ("rnmos", RNMOS_KW),
    ("rpmos", RPMOS_KW),
    ("rtran", RTRAN_KW),
    ("rtranif0", RTRANIF0_KW),
    ("rtranif1", RTRANIF1_KW),
    ("s_always", S_ALWAYS_KW),
    ("s_eventually", S_EVENTUALLY_KW),
    ("s_nexttime", S_NEXTTIME_KW),
    ("s_until", S_UNTIL_KW),
    ("s_until_with", S_UNTIL_WITH_KW),
    ("scalared", SCALARED_KW),
    ("sequence", SEQUENCE_KW),
    ("shortint", SHORTINT_KW),
    ("shortreal", SHORTREAL_KW),
    ("showcancelled", SHOWCANCELLED_KW),
    ("signed", SIGNED_KW),
    ("small", SMALL_KW),
    ("soft", SOFT_KW),
    ("solve", SOLVE_KW),
    ("specify", SPECIFY_KW),
    ("specparam", SPECPARAM_KW),
    ("static", STATIC_KW),
    ("string", STRING_KW),
    ("strong", STRONG_KW),
    ("strong0", STRONG0_KW),
    ("strong1", STRONG1_KW),
    ("struct", STRUCT_KW),
    ("super", SUPER_KW),
    ("supply0", SUPPLY0_KW),
    ("supply1", SUPPLY1_KW),
    ("sync_accept_on", SYNC_ACCEPT_ON_KW),
    ("sync_reject_on", SYNC_REJECT_ON_KW),
    ("table", TABLE_KW),
    ("tagged", TAGGED_KW),
    ("task", TASK_KW),
    ("this", THIS_KW),
    ("throughout", THROUGHOUT_KW),
    ("time", TIME_KW),
    ("timeprecision", TIMEPRECISION_KW),
    ("timeunit", TIMEUNIT_KW),
    ("tran", TRAN_KW),
    ("tranif0", TRANIF0_KW),
    ("tranif1", TRANIF1_KW),
    ("tri", TRI_KW),
    ("tri0", TRI0_KW),
    ("tri1", TRI1_KW),
    ("triand", TRIAND_KW),
    ("trior", TRIOR_KW),
    ("trireg", TRIREG_KW),
    ("type", TYPE_KW),
    ("typedef", TYPEDEF_KW),
    ("union", UNION_KW),
    ("unique", UNIQUE_KW),
    ("unique0", UNIQUE0_KW),
    ("unsigned", UNSIGNED_KW),
    ("until", UNTIL_KW),
    ("until_with", UNTIL_WITH_KW),
    ("untyped", UNTYPED_KW),
    ("use", USE_KW),
    ("uwire", UWIRE_KW),
    ("var", VAR_KW),
    ("vectored", VECTORED_KW),
    ("virtual", VIRTUAL_KW),
    ("void", VOID_KW),
    ("wait", WAIT_KW),
    ("wait_order", WAIT_ORDER_KW),
    ("wand", WAND_KW),
    ("weak", WEAK_KW),
    ("weak0", WEAK0_KW),
    ("weak1", WEAK1_KW),
    ("while", WHILE_KW),
    ("wildcard", WILDCARD_KW),
    ("wire", WIRE_KW),
    ("with", WITH_KW),
    ("within", WITHIN_KW),
    ("wor", WOR_KW),
    ("xnor", XNOR_KW),
    ("xor", XOR_KW),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted() {
        for pair in KEYWORDS_1800_2023.windows(2) {
            assert!(pair[0].0 < pair[1].0, "{} then {}", pair[0].0, pair[1].0);
        }
    }

    #[test]
    fn table_round_trips() {
        for &(text, kind) in KEYWORDS_1800_2023 {
            assert_eq!(lookup(text, KeywordVersion::default()), Some(kind));
            assert_eq!(kind.keyword_text(), Some(text));
            assert!(kind.is_keyword(), "{text}");
        }
    }

    #[test]
    fn keyword_block_is_contiguous() {
        // `is_keyword` is a range check over the enum, which is only sound while
        // the keyword variants stay in one run. Inserting a non-keyword among them
        // fails here rather than silently.
        let first = KEYWORDS_1800_2023
            .iter()
            .map(|&(_, k)| k as u16)
            .min()
            .unwrap();
        let last = KEYWORDS_1800_2023
            .iter()
            .map(|&(_, k)| k as u16)
            .max()
            .unwrap();
        assert_eq!(
            (last - first + 1) as usize,
            KEYWORDS_1800_2023.len(),
            "the keyword variants are no longer one contiguous run"
        );
    }

    #[test]
    fn non_keywords_are_not_keywords() {
        for text in ["foo", "logicx", "xlogic", "", "Module", "clk_i"] {
            assert_eq!(lookup(text, KeywordVersion::default()), None, "{text}");
        }
        for kind in [IDENT, WHITESPACE, L_PAREN, STRING_LITERAL, EOF] {
            assert!(!kind.is_keyword(), "{kind:?}");
            assert_eq!(kind.keyword_text(), None);
        }
    }
}
