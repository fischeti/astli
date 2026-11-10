// Not reported: `CrossQueueType` is the type a cross declares for the
// functions in its body that return its bins.
module m;
  int c, d;
  covergroup cg (int lim);
    coverpoint c;
    coverpoint d;
    cXd: cross c, d {
      function CrossQueueType pairs(int n);
      endfunction
      bins one = pairs(lim);
    }
  endgroup
endmodule
