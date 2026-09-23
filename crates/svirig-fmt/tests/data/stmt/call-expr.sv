// A call takes no space before its `(` or inside it. One that does not fit
// packs its arguments under the first; where they would still pass the width,
// or start past half of it, it breaks after `(`, packs them a continuation
// in, and closes on a line of its own.
module m;
  logic [$clog2( DEPTH )-1:0] ptr;
  initial begin
    $display ( "done" );
    foo();
    $display("%0d %0d %0d %0d", first_counter_value, second_counter_value, third_counter_value, fourth);
    result_of_a_rather_long_computation = compute_something_with_a_long_name(argument_number_one, argument_number_two_which_is_long, three);
    result_of_a_rather_long_computation = compute_something_with_a_long_name(argument_number_one, argument_number_two, three);
    x = outer_function(inner_function(alpha_argument, beta_argument), gamma_argument, delta_argument_long);
    y = f(a, // why a
          b);
  end
endmodule
