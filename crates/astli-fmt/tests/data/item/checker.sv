checker handshake(logic clk,untyped req,sequence ack=req ##1 req,output bit ok=1'b0);
default clocking @(posedge clk); endclocking
  a_ack: assert property (ack);
always_ff @(posedge clk) ok<=req;
endchecker:handshake
module m;
  always @(posedge clk) begin
    if (req) handshake u_in_proc(clk,req);
  end
  pkg::stable u_stable(posedge clk,req);
endmodule
