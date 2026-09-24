// The import gives back only its own tokens: rolling back the whole shell
// would cross a marker that is still open.
module m import pkg::*
endmodule
