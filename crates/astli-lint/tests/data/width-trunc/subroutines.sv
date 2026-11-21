// A function's or a task's body is sized in each body that builds it, its
// declarations' initialisers among it; nothing in a class is.
module top (
  input logic [7:0] a_i,
  output logic [3:0] y_o
);
  function automatic logic [3:0] low(input logic [3:0] value);
    logic [3:0] wide = a_i;
    logic [3:0] narrow;
    narrow = a_i;
    return value;
  endfunction

  task automatic count(output logic [3:0] n);
    n = a_i;
  endtask

  class c;
    logic [3:0] n = 8'hff;
  endclass

  assign y_o = low(4'd1);
endmodule
