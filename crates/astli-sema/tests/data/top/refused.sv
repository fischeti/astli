// What a top cannot be given: a parameter it does not declare, a
// `localparam`, a body `parameter` its parameter port list makes local, a
// type parameter, and a top that is no definition. A literal sets a
// parameter as a simulator's `-G` does, its text as written.
module listed #(parameter bit M = 0);
  parameter int Depth = 4;
  localparam int Twice = 8;
endmodule

module empty_list #();
  parameter int Depth = 4;
endmodule

module bare;
  parameter int Depth = 4;
  localparam int Twice = Depth * 2;
endmodule

//> set listed Depth=1
//> literal listed Depth=1
//> set empty_list Depth=1
//> set listed Twice=1
//> set bare Twice=1
//> set bare Absent=1
//> set nowhere Depth=1
//> literal bare Depth=8'd200
//> literal bare Depth=-3
//> literal bare Depth=x
//> set bare Depth=8'd200
//> values listed M
