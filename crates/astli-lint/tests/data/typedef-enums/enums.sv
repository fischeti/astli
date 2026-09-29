// An enum is declared through a typedef; one written in a variable's,
// parameter's or member's type is reported.
package enums;
  typedef enum logic { Off, On } mode_e;
  typedef struct packed {
    enum logic { Lo, Hi } level;
  } pin_t;
endpackage

module state;
  enum logic [1:0] { Idle, Busy } state_q;
  mode_e mode_q;
endmodule
