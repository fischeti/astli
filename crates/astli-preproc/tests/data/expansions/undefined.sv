// An undefined macro records no expansion, and stands as written. A macro
// that reaches itself expands once, and the call its body writes stands.
`define SELF `SELF
assign y = `UNDEFINED + `SELF;
