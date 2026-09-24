// Each of these keywords only sometimes opens a body. A wrong guess would
// reach the end of the file unclosed, so none of these may report.
module top;
  extern function void f();
endmodule
class C;
  pure virtual function void f();
endclass
module top;
  virtual interface i_if vif;
endmodule
typedef class C;
module top;
  assert property (@(posedge c) a |-> b);
endmodule
module top;
  import "DPI-C" function void f();
endmodule
interface class IC;
  pure virtual function void g();
endclass
module top;
  sequence s; a ##1 b; endsequence
endmodule
module top;
  property p; a |-> b; endproperty
endmodule
package p;
  typedef enum { A, B } e_t;
endpackage
