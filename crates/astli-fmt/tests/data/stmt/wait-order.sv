module m;
  initial begin
    wait_order(e1,e2,e3)$display("in order");else $error("out of order");
    wait_order( e1,top.u.e2 );
    wait_order (e1, e2) else $error("out of order");
  end
endmodule
