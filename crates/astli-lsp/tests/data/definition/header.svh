// A place in a header is its own, asked through the unit that includes it.
typedef enum logic { Idle, Busy } state_t;

function automatic state_t next(state_t now);
//                 ^              ^
  return now == Idle ? Busy : Idle;
//       ^      ^
endfunction
