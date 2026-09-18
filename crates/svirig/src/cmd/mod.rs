//! One module per subcommand. Each takes the parsed arguments and does the
//! work; none of them mentions the argument parser.

pub mod completion;
pub mod fmt;
pub mod lex;
pub mod parse;
pub mod preprocess;
