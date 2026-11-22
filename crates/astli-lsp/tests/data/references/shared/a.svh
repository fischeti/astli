// A header both units include.
typedef logic [7:0] shared_byte_t;
//                  ^
function automatic shared_byte_t shared_inc(shared_byte_t v);
  return v + 1;
endfunction
