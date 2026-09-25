// Consecutive `assign`s line up on their `=`, and so do consecutive
// statements with one operator; any other item ends the run. Only the first
// assignment of an `assign` lines up. A column lines up only among
// consecutive rows that need at most twelve spaces of padding for it. Named
// connections line up however far one sticks out, as the guide requires.
module m;
  assign a = b;
  assign ready_q = valid_d, c = d;
  assign \esc = e;
  assign a_target_far_longer_than_the_rest = f;
  assign g = h;

  always_ff @(posedge clk_i) begin
    q <= d;
    count_q <= count_d;
    x = 1;
    longer = 2;
    y += 1;
    zz -= 1;
    $display("x");
    s <= t;
  end

  mod u_mod (
    .a(a),
    .b(b),
    .a_connection_far_longer_than_the_rest(c)
  );
endmodule
