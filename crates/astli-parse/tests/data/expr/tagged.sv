// A tagged union's value: the member, and what it holds unless it is `void`.
// The member is a bare name, so its value is not the arguments of a call.
module m;
  initial begin
    a = tagged Invalid;
    b = tagged Valid(42);
    c = tagged Valid (23 + 34) + 1;
    d = tagged Add '{e, f};
    g = tagged Jmp (tagged JmpU 239);
    h = tagged \esc x.y[0];
  end
endmodule
