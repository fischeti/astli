// A parameter named for what it turns off is reported, in any case.
module params #(
  parameter bit EnableParity   = 1'b1,
  parameter bit DisableParity  = 1'b0,
  parameter bit DISABLE_CHECKS = 1'b0
) ();
  localparam bit disableFoo = 1'b0;
  localparam bit Disabled   = 1'b0;
endmodule
