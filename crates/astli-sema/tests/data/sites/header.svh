// What a header declares is found through the unit that includes it,
// and a place in the header is its own.
typedef enum logic {
  Idle,
  Busy
} state_t;
//^

function automatic state_t next(state_t now);
//                              ^       ^
  return now == Idle ? Busy : Idle;
//       ^      ^
endfunction
