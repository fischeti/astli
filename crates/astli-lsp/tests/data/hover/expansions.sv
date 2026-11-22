// What a macro call expands to is laid out as what it parses as: module
// items, statements, an expression, a data type, ports or parameters. One
// that parses as none of them is shown as written, without the spaces each
// `\` left and the indentation its lines share, and cut at a line's end.

`define CHAN_T(Width)                  \
    struct packed {                    \
      logic [2:0]       opcode;        \
      logic [Width-1:0] address;       \
    }

`define REQ_T(Width)           \
  struct packed {              \
    `CHAN_T(Width) a;          \
    logic          a_valid;    \
    logic          d_ready;    \
  }

`define FIELDS logic first; logic [7:0] second;

`define WIDEST(a, b) ((a) > (b) ? (a) : (b))

`define CLEAR(q)          \
  if (!rst_ni) begin      \
    q <= '0;              \
  end

`define BUS_PORTS(p)          \
  input  logic       p``_valid, \
  output logic       p``_ready, \
  input  logic [7:0] p``_data

`define BUS_PARAMS                  \
  parameter int  Width = 8,         \
  parameter type data_t = logic [Width-1:0]

`define REGISTERS_BEGIN(name)                                          \
  always_ff @(posedge clk_i) begin : name                             \
      first_register_with_a_long_name  <= first_register_with_a_long_name_d;  \
      second_register_with_a_long_name <= second_register_with_a_long_name_d; \
      third_register_with_a_long_name  <= third_register_with_a_long_name_d;  \
      fourth_register_with_a_long_name <= fourth_register_with_a_long_name_d; \
      fifth_register_with_a_long_name  <= fifth_register_with_a_long_name_d;  \
      sixth_register_with_a_long_name  <= sixth_register_with_a_long_name_d;

module sub #(
  `BUS_PARAMS
//^
) (
  `BUS_PORTS(in)
//^
);
endmodule

module top;
  localparam int AddrWidth = 32;
  localparam type req_t = `REQ_T(AddrWidth);
//                        ^
  typedef struct packed {`FIELDS} fields_t;
//                       ^
  localparam int FirstWidthParameter = 8;
  localparam int SecondWidthParameter = 16;
  localparam int Widest = `WIDEST(FirstWidthParameter, SecondWidthParameter);
//                        ^
  logic clk_i, rst_ni;
  logic [7:0] q;
  always_ff @(posedge clk_i) begin
    `CLEAR(q)
//  ^
  end
  logic first_register_with_a_long_name, first_register_with_a_long_name_d;
  logic second_register_with_a_long_name, second_register_with_a_long_name_d;
  logic third_register_with_a_long_name, third_register_with_a_long_name_d;
  logic fourth_register_with_a_long_name, fourth_register_with_a_long_name_d;
  logic fifth_register_with_a_long_name, fifth_register_with_a_long_name_d;
  logic sixth_register_with_a_long_name, sixth_register_with_a_long_name_d;
  `REGISTERS_BEGIN(regs)
//^
  end
endmodule
