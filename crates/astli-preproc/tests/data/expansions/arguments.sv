// A call over several lines, with defaults, an empty argument and nested
// parentheses, spans all of it.
`define CHECK(cond, msg = "failed", sev = $error) \
  if (!(cond)) sev(msg);
initial begin
  `CHECK((a && (b || c)), , $fatal)
  `CHECK(
    x == {y, z},
    "x is not y and z"
  )
end
