// An enum type is lower_snake_case ending in `_e`, or `_t` as any type. An
// anonymous enum has no type name to check, and other typedefs are not this
// rule's.
package enums;
  typedef enum logic [1:0] { Idle, Busy } state_e;
  typedef enum logic { Off, On }          mode_t;
  typedef enum logic { A, B }             StateE;
  typedef enum logic { C, D }             phase;
  typedef logic [7:0]                     byte_e;
  enum logic { E, F } anonymous;
endpackage
