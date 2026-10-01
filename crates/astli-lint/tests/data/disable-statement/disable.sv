// `disable` of a `begin` block around the statement is fine, and so is
// `disable fork`; a fork's label, a task, a block elsewhere and a process's
// whole body are reported.
module m;
  task automatic t;
  endtask
  initial begin
    begin : search
      for (int i = 0; i < 4; i++) if (i == 2) disable search;
    end
    fork : workers
      #1;
      #2;
    join_none
    disable workers;
    disable fork;
    disable t;
  end
  initial begin : whole
    disable whole;
  end
  initial labelled: begin
    begin : inner
      disable inner;
    end
  end
endmodule
