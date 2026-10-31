// Without tops, a definition is checked as its defaults build it, and what
// typing rejects there is a warning, as other parameters may build it
// otherwise; a generate arm its defaults do not build is not checked. A
// package, and the classes and functions it declares, has one body, and
// what typing rejects there is an error. An associative array, and a
// foreach's variable, which has the type of the array's index, have none
// sema knows, so nothing is said of them.
package p;
  typedef enum logic [1:0] {Idle, Busy} state_e;

  class tracker;
    state_e state;
    string names [string];
    int count;

    function void step(logic [1:0] raw);
      state = raw;
      count = names;
      foreach (names[key]) begin
        string copy = key;
      end
    endfunction
  endclass
endpackage

module m import p::*; #(parameter int Width = 2) (input logic [Width-1:0] raw);
  state_e state;
  if (Width == 2) begin : g_narrow
    assign state = raw;
  end else begin : g_wide
    logic [Width-1:0] unused;
    assign state = unused[0] ? Idle : 3'd1;
  end
endmodule
